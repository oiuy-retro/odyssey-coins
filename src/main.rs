mod byml;
mod msbt;
mod sarc;
mod yaz0;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use byml::Value;

struct Placement {
    unique_id: i64,
    stage: String,
    obj_id: String,
}

struct World {
    code: String,
    prefix: String,
    stages: Vec<String>,
    expected: i64,
}

struct Placed {
    x: f32,
    y: f32,
    z: f32,
}

struct Checkpoint {
    id: String,
    name: String,
    x: f32,
    y: f32,
    z: f32,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("odyssey-coins: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut save: Option<PathBuf> = None;
    let mut content: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--save" => {
                i += 1;
                save = Some(PathBuf::from(need_arg(&args, i, "--save")?));
            }
            "--content" => {
                i += 1;
                content = Some(PathBuf::from(need_arg(&args, i, "--content")?));
            }
            other => return Err(format!("unknown argument {other}. Use --help.")),
        }
        i += 1;
    }
    let Some(save) = save else {
        print_help();
        return Err("pass --save with your File2.bin".into());
    };
    let got = read_save(&save)?;
    let collected = got.values().filter(|g| **g).count();
    let missing_n = got.len() - collected;
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "Save: {collected} collected, {missing_n} missing, {} listed",
        got.len()
    )
    .ok();
    let Some(content) = content else {
        writeln!(
            out,
            "Pass --content with your extracted game folder to name the kingdoms and show places."
        )
        .ok();
        writeln!(out, "Missing ids: {}", ranges(&missing_ids(&got))).ok();
        return Ok(());
    };
    let world_list = find_world_list(&content)?;
    let archive = open_szs(&world_list)?;
    let files = sarc::read(&archive)?;
    let by_name = files
        .iter()
        .map(|f| (f.name.as_str(), f.data.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let placements = read_placements(by_name.get("CoinCollectList.byml").copied())?;
    let worlds = read_worlds(&by_name)?;
    let messages = load_messages(&content).unwrap_or_default();
    let mut by_world: Vec<Vec<&Placement>> = worlds.iter().map(|_| Vec::new()).collect();
    let mut unmatched = Vec::new();
    for place in &placements {
        match world_index(&worlds, &place.stage) {
            Some(idx) => by_world[idx].push(place),
            None => unmatched.push(place),
        }
    }
    let mut stage_cache: BTreeMap<String, StageInfo> = BTreeMap::new();
    for (wi, world) in worlds.iter().enumerate() {
        let coins = &by_world[wi];
        if coins.is_empty() && world.expected == 0 {
            continue;
        }
        let total = coins.len();
        let have = coins
            .iter()
            .filter(|c| got.get(&c.unique_id).copied().unwrap_or(false))
            .count();
        let miss: Vec<&&Placement> = coins
            .iter()
            .filter(|c| !got.get(&c.unique_id).copied().unwrap_or(false))
            .collect();
        writeln!(
            out,
            "\n{} - {} missing ({have}/{total})",
            display_world(&world.code),
            miss.len()
        )
        .ok();
        if world.expected > 0 && world.expected as usize != total {
            writeln!(
                out,
                "  (the game list says {} coins for this kingdom; {} were matched)",
                world.expected, total
            )
            .ok();
        }
        for coin in miss {
            writeln!(out, "  {}  {}", coin.stage, coin.obj_id).ok();
            let (place_stage, place_id) = linked_placement(&coin.stage, &coin.obj_id);
            let info = stage_info(&content, place_stage, &messages, &mut stage_cache);
            match info.coins.get(place_id) {
                Some(pos) => {
                    writeln!(
                        out,
                        "    position {:.0}, {:.0}, {:.0}",
                        pos.x, pos.y, pos.z
                    )
                    .ok();
                    if place_stage != coin.stage {
                        writeln!(out, "    inside {place_stage}").ok();
                    }
                    if let Some(flag) = nearest(&info.checkpoints, pos) {
                        let dx = pos.x - flag.x;
                        let dy = pos.y - flag.y;
                        let dz = pos.z - flag.z;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        writeln!(
                            out,
                            "    nearest checkpoint: {} ({}), offset {:.0}, {:.0}, {:.0} ({:.0} away)",
                            flag.name, flag.id, dx, dy, dz, dist
                        )
                        .ok();
                    }
                }
                None => {
                    if info.loaded {
                        writeln!(out, "    not placed in that stage's map").ok();
                    } else {
                        writeln!(
                            out,
                            "    stage map was not in the folder you passed to --content"
                        )
                        .ok();
                    }
                }
            }
        }
    }
    if !unmatched.is_empty() {
        let miss = unmatched
            .iter()
            .filter(|c| !got.get(&c.unique_id).copied().unwrap_or(false))
            .count();
        writeln!(
            out,
            "\nUnmatched stages - {miss} missing ({} placed)",
            unmatched.len()
        )
        .ok();
    }
    let known: std::collections::BTreeSet<i64> = placements.iter().map(|p| p.unique_id).collect();
    let extra: Vec<i64> = got
        .iter()
        .filter(|(id, got)| !**got && !known.contains(id))
        .map(|(id, _)| *id)
        .collect();
    if !extra.is_empty() {
        writeln!(
            out,
            "\nMissing in the save but not in the coin list you supplied: {}",
            ranges(&extra)
        )
        .ok();
    }
    Ok(())
}

fn print_help() {
    println!(
        "\
Odyssey Coins

Lists kingdom coins your save has not collected. This program does not
include the game. You point it at your own save, and, for places, at an
extracted copy of the game that stays on your computer.

Usage
  odyssey-coins --save FILE [--content DIR]

  --save     File2.bin from your Eden save for this game
  --content  extracted game folder (the romfs root)

The game folder is expected to contain:
  SystemData/WorldList.szs
  StageData/<stage>Map.szs
  LocalizedData/USen/MessageData/StageMessage.szs   (optional checkpoint names)

Nothing from that folder is written anywhere by this program."
    );
}

fn need_arg(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a path"))
}

fn read_save(path: &Path) -> Result<BTreeMap<i64, bool>, String> {
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let doc = if bytes.len() >= 2 && (&bytes[..2] == b"YB" || &bytes[..2] == b"BY") {
        byml::parse(&bytes)?
    } else if bytes.len() > 18 && (&bytes[16..18] == b"YB" || &bytes[16..18] == b"BY") {
        byml::parse(&bytes[16..])?
    } else {
        return Err("that file does not look like an Odyssey save".into());
    };
    let list = doc
        .get("GetCoinCollectData")
        .and_then(Value::as_array)
        .ok_or("save has no coin collection list")?;
    let mut out = BTreeMap::new();
    for entry in list {
        let id = entry
            .get("UniqueId")
            .and_then(Value::as_int)
            .ok_or("a coin entry has no UniqueId")?;
        let is_get = entry.get("IsGet").and_then(Value::as_bool).unwrap_or(false);
        out.insert(id, is_get);
    }
    if out.is_empty() {
        return Err("the coin list in that save is empty".into());
    }
    Ok(out)
}

fn find_world_list(content: &Path) -> Result<PathBuf, String> {
    let candidates = [
        content.join("SystemData").join("WorldList.szs"),
        content.join("WorldList.szs"),
    ];
    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    Err(
        "could not find SystemData/WorldList.szs in the folder you passed to --content".into(),
    )
}

fn open_szs(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    yaz0::maybe_decompress(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_placements(bytes: Option<&[u8]>) -> Result<Vec<Placement>, String> {
    let bytes = bytes.ok_or("WorldList archive has no coin list")?;
    let doc = byml::parse(bytes)?;
    let table = doc
        .get("Table")
        .and_then(Value::as_array)
        .ok_or("coin list has no Table")?;
    let mut out = Vec::with_capacity(table.len());
    for row in table {
        let unique_id = row
            .get("UniqueId")
            .and_then(Value::as_int)
            .ok_or("coin list row has no UniqueId")?;
        let stage = row
            .get("StageName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let obj_id = row
            .get("ObjId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        out.push(Placement {
            unique_id,
            stage,
            obj_id,
        });
    }
    Ok(out)
}

fn read_worlds(files: &BTreeMap<&str, &[u8]>) -> Result<Vec<World>, String> {
    let doc_bytes = files
        .get("WorldListFromDb.byml")
        .copied()
        .or_else(|| files.get("WorldList.byml").copied())
        .ok_or("WorldList archive has no world list")?;
    let doc = byml::parse(doc_bytes)?;
    let rows = doc
        .as_array()
        .ok_or("world list is not a list")?;
    let counts = collect_counts(files.get("CollectCoinNum.byml").copied())?;
    let mut worlds = Vec::new();
    for row in rows {
        let code = row
            .get("WorldName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let home = row.get("Name").and_then(Value::as_str).unwrap_or("");
        let prefix = home.strip_suffix("HomeStage").unwrap_or(home).to_string();
        let mut stages = Vec::new();
        if let Some(list) = row.get("StageList").and_then(Value::as_array) {
            for stage in list {
                if let Some(name) = stage.get("name").and_then(Value::as_str) {
                    stages.push(name.to_string());
                }
            }
        }
        let expected = counts.get(&code).copied().unwrap_or(0);
        worlds.push(World {
            code,
            prefix,
            stages,
            expected,
        });
    }
    Ok(worlds)
}

fn collect_counts(bytes: Option<&[u8]>) -> Result<BTreeMap<String, i64>, String> {
    let mut out = BTreeMap::new();
    let Some(bytes) = bytes else {
        return Ok(out);
    };
    let doc = byml::parse(bytes)?;
    let Some(rows) = doc.as_array() else {
        return Ok(out);
    };
    for row in rows {
        let Some(name) = row.get("WorldName").and_then(Value::as_str) else {
            continue;
        };
        let n = row
            .get("CollectCoinNum")
            .and_then(Value::as_int)
            .unwrap_or(0);
        out.insert(name.to_string(), n);
    }
    Ok(out)
}

fn world_index(worlds: &[World], stage: &str) -> Option<usize> {
    worlds.iter().position(|w| w.stages.iter().any(|s| s == stage)).or_else(|| {
        let mut best: Option<(usize, usize)> = None;
        for (i, world) in worlds.iter().enumerate() {
            if world.prefix.is_empty() || !stage.starts_with(&world.prefix) {
                continue;
            }
            let len = world.prefix.len();
            if best.map(|(_, n)| len > n).unwrap_or(true) {
                best = Some((i, len));
            }
        }
        best.map(|(i, _)| i)
    })
}

struct StageInfo {
    loaded: bool,
    coins: BTreeMap<String, Placed>,
    checkpoints: Vec<Checkpoint>,
}

fn linked_placement<'a>(stage: &'a str, obj_id: &'a str) -> (&'a str, &'a str) {
    // Some list entries name a coin that lives in a linked area:
    // obj380(SnowWorldIcicleZone[obj417])
    let Some((id, rest)) = obj_id.split_once('(') else {
        return (stage, obj_id);
    };
    let Some((zone, _)) = rest.split_once('[') else {
        return (stage, obj_id);
    };
    if id.starts_with("obj") && !zone.is_empty() && rest.ends_with("])") {
        (zone, id)
    } else {
        (stage, obj_id)
    }
}

fn stage_info<'a>(
    content: &Path,
    stage: &str,
    messages: &'a BTreeMap<String, BTreeMap<String, String>>,
    cache: &'a mut BTreeMap<String, StageInfo>,
) -> &'a StageInfo {
    if !cache.contains_key(stage) {
        let info = load_stage(content, stage, messages);
        cache.insert(stage.to_string(), info);
    }
    cache.get(stage).unwrap()
}

fn load_stage(
    content: &Path,
    stage: &str,
    messages: &BTreeMap<String, BTreeMap<String, String>>,
) -> StageInfo {
    let path = content
        .join("StageData")
        .join(format!("{stage}Map.szs"));
    let names = messages.get(stage);
    let Ok(archive) = open_szs(&path) else {
        return StageInfo {
            loaded: false,
            coins: BTreeMap::new(),
            checkpoints: Vec::new(),
        };
    };
    let Ok(files) = sarc::read(&archive) else {
        return StageInfo {
            loaded: false,
            coins: BTreeMap::new(),
            checkpoints: Vec::new(),
        };
    };
    let want = format!("{stage}Map.byml");
    let Some(map) = files.iter().find(|f| f.name == want) else {
        return StageInfo {
            loaded: true,
            coins: BTreeMap::new(),
            checkpoints: Vec::new(),
        };
    };
    let Ok(doc) = byml::parse(&map.data) else {
        return StageInfo {
            loaded: true,
            coins: BTreeMap::new(),
            checkpoints: Vec::new(),
        };
    };
    let mut actors = Vec::new();
    walk_actors(&doc, &mut actors);
    let mut coins = BTreeMap::new();
    let mut checkpoints = Vec::new();
    for actor in actors {
        let Some(pos) = actor.pos else { continue };
        if actor.unit.contains("CoinCollect") {
            coins.entry(actor.id.clone()).or_insert(pos);
        } else if actor.unit == "CheckpointFlag" {
            let name = names
                .and_then(|m| m.get(&actor.id))
                .cloned()
                .unwrap_or_else(|| actor.id.clone());
            checkpoints.push(Checkpoint {
                id: actor.id,
                name,
                x: pos.x,
                y: pos.y,
                z: pos.z,
            });
        }
    }
    StageInfo {
        loaded: true,
        coins,
        checkpoints,
    }
}

struct Actor {
    unit: String,
    id: String,
    pos: Option<Placed>,
}

fn walk_actors(value: &Value, out: &mut Vec<Actor>) {
    match value {
        Value::Dict(dict) => {
            if let (Some(unit), Some(id)) = (
                dict.get("UnitConfigName").and_then(Value::as_str),
                dict.get("Id").and_then(Value::as_str),
            ) {
                out.push(Actor {
                    unit: unit.to_string(),
                    id: id.to_string(),
                    pos: translate(dict.get("Translate")),
                });
            }
            for child in dict.values() {
                walk_actors(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk_actors(child, out);
            }
        }
        _ => {}
    }
}

fn translate(value: Option<&Value>) -> Option<Placed> {
    let value = value?;
    let x = value.get("X").and_then(Value::as_float)?;
    let y = value.get("Y").and_then(Value::as_float)?;
    let z = value.get("Z").and_then(Value::as_float)?;
    Some(Placed { x, y, z })
}

fn nearest<'a>(flags: &'a [Checkpoint], pos: &Placed) -> Option<&'a Checkpoint> {
    flags.iter().min_by(|a, b| {
        let da = dist(a, pos);
        let db = dist(b, pos);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn dist(flag: &Checkpoint, pos: &Placed) -> f32 {
    let dx = pos.x - flag.x;
    let dy = pos.y - flag.y;
    let dz = pos.z - flag.z;
    dx * dx + dy * dy + dz * dz
}

fn load_messages(content: &Path) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let path = content
        .join("LocalizedData")
        .join("USen")
        .join("MessageData")
        .join("StageMessage.szs");
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let archive = open_szs(&path)?;
    let files = sarc::read(&archive)?;
    let mut out = BTreeMap::new();
    for file in files {
        let Some(stage) = file.name.strip_suffix(".msbt") else {
            continue;
        };
        if let Ok(names) = msbt::checkpoint_names(&file.data) {
            if !names.is_empty() {
                out.insert(stage.to_string(), names);
            }
        }
    }
    Ok(out)
}

fn missing_ids(got: &BTreeMap<i64, bool>) -> Vec<i64> {
    got.iter()
        .filter(|(_, is_get)| !**is_get)
        .map(|(id, _)| *id)
        .collect()
}

fn ranges(ids: &[i64]) -> String {
    if ids.is_empty() {
        return "(none)".into();
    }
    let mut parts = Vec::new();
    let mut start = ids[0];
    let mut prev = ids[0];
    for &id in ids.iter().skip(1) {
        if id == prev + 1 {
            prev = id;
            continue;
        }
        parts.push(range_text(start, prev));
        start = id;
        prev = id;
    }
    parts.push(range_text(start, prev));
    parts.join(", ")
}

fn range_text(start: i64, end: i64) -> String {
    if start == end {
        format!("{start}")
    } else {
        format!("{start}-{end}")
    }
}

fn display_world(code: &str) -> String {
    let label = match code {
        "Cap" => "Cap Kingdom",
        "Waterfall" => "Cascade Kingdom",
        "Sand" => "Sand Kingdom",
        "Lake" => "Lake Kingdom",
        "Forest" => "Wooded Kingdom",
        "Cloud" => "Cloud Kingdom",
        "Clash" => "Lost Kingdom",
        "City" => "Metro Kingdom",
        "Snow" => "Snow Kingdom",
        "Sea" => "Seaside Kingdom",
        "Lava" => "Luncheon Kingdom",
        "Sky" => "Bowser's Kingdom",
        "Moon" => "Moon Kingdom",
        "Peach" => "Mushroom Kingdom",
        "Special1" => "Dark Side",
        "Special2" => "Darker Side",
        "BossRaid" => "Ruined Kingdom",
        _ => "",
    };
    if label.is_empty() {
        code.to_string()
    } else {
        label.to_string()
    }
}
