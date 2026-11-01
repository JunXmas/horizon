//! Obfuscated releases (1.21.11 and older) remapped to Mojang's names, as tools/remap.py does for
//! the dev tools: Mojang's ProGuard mappings (`client_mappings`, `server_mappings`, checked against
//! the version JSON's sha1) as tiny v1, then the tiny-remapper research-lock.json pins, run with the
//! Java the preparation already found. The mappings and the remapper are fetched once into
//! `.local/tools`, each checked; the game files themselves stay the player's own.
use crate::pyjson::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const PRIMITIVES: [(&str, &str); 9] = [
    ("void", "V"),
    ("boolean", "Z"),
    ("byte", "B"),
    ("char", "C"),
    ("short", "S"),
    ("int", "I"),
    ("long", "J"),
    ("float", "F"),
    ("double", "D"),
];

/// `net.minecraft.Foo[]` as a descriptor in the obfuscated namespace (`classes`: named -> official).
fn descriptor(java_type: &str, classes: &BTreeMap<&str, &str>) -> String {
    let dims = java_type.matches("[]").count();
    let base = java_type.trim_end_matches("[]");
    let element = match PRIMITIVES.iter().find(|(name, _)| *name == base) {
        Some((_, code)) => (*code).to_owned(),
        None => format!(
            "L{};",
            classes.get(base).copied().unwrap_or(base).replace('.', "/")
        ),
    };
    format!("{}{element}", "[".repeat(dims))
}

/// `a.b.C -> x:` as (named, official).
fn class_line(line: &str) -> Option<(&str, &str)> {
    let (named, official) = line.strip_suffix(':')?.split_once(" -> ")?;
    (!line.starts_with(char::is_whitespace)).then_some((named, official))
}

/// Mojang's ProGuard mappings as tiny v1 text, official -> named.
pub fn proguard_to_tiny(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let classes: BTreeMap<&str, &str> = lines.iter().filter_map(|l| class_line(l)).collect();
    let mut out = String::from("v1\tofficial\tnamed\n");
    let mut owner: Option<String> = None;
    let mut seen = BTreeSet::new();
    for line in lines {
        if let Some((named, official)) = class_line(line) {
            let official = official.replace('.', "/");
            out += &format!("CLASS\t{official}\t{}\n", named.replace('.', "/"));
            owner = Some(official);
            continue;
        }
        let unreadable = || format!("Unreadable mapping line: {line:?}");
        let owner = owner.as_deref().ok_or_else(unreadable)?;
        let (left, obf) = line.trim().rsplit_once(" -> ").ok_or_else(unreadable)?;
        // Line ranges ("1:2:") before the type, and after the arguments for inlined frames.
        let left = left.trim_start_matches(|c: char| c.is_ascii_digit() || c == ':');
        let (java_type, rest) = left.split_once(' ').ok_or_else(unreadable)?;
        let (name, args) = match rest.split_once('(') {
            Some((name, args)) => (name, Some(args.split(')').next().unwrap_or(""))),
            None => (rest, None),
        };
        // An inlined frame of another class's method, not a member of this one.
        if name.contains('.') {
            continue;
        }
        let row = match args {
            None => format!(
                "FIELD\t{owner}\t{}\t{obf}\t{name}",
                descriptor(java_type, &classes)
            ),
            Some(args) => {
                let params: String = args
                    .split(',')
                    .filter(|a| !a.is_empty())
                    .map(|a| descriptor(a, &classes))
                    .collect();
                format!(
                    "METHOD\t{owner}\t({params}){}\t{obf}\t{name}",
                    descriptor(java_type, &classes)
                )
            }
        };
        // Line ranges repeat a method once per range.
        if seen.insert(row.clone()) {
            out += &row;
            out.push('\n');
        }
    }
    Ok(out)
}

fn fetch(
    url: &str,
    destination: &Path,
    checksum: &str,
    algorithm: &str,
) -> Result<PathBuf, String> {
    if destination.exists() && crate::digest(destination, algorithm)? == checksum {
        return Ok(destination.to_path_buf());
    }
    let response = horizon_port_pack::net::agent(url)
        .get(url)
        .call()
        .map_err(|e| format!("Downloading {url}: {e}"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response.into_reader(), &mut bytes)
        .map_err(|e| format!("Downloading {url}: {e}"))?;
    let partial = destination.with_extension("partial");
    crate::mkdir(destination.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(&partial, &bytes).map_err(|e| format!("{}: {e}", partial.display()))?;
    let actual = crate::digest(&partial, algorithm)?;
    if actual != checksum {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "{url} does not match its checksum: expected {checksum}, got {actual}"
        ));
    }
    std::fs::rename(&partial, destination)
        .map_err(|e| format!("{}: {e}", destination.display()))?;
    Ok(destination.to_path_buf())
}

/// The imported jars of an obfuscated version as `<side>-named.jar` beside them, before
/// `artifacts::verify` looks for those; nothing to do for a version that is not obfuscated or not
/// imported yet (verify then says what is missing).
pub fn remap_imported(
    root: &Path,
    lock: &crate::artifacts::Lock,
    jdk: &crate::java::Jdk,
    log: crate::Log,
) -> Result<(), String> {
    let manifest_path = lock.directory.join("manifest.json");
    let jars = lock.directory.join("jars");
    if !manifest_path.exists()
        || !jars.join("client.jar").exists()
        || !jars.join("server-game.jar").exists()
    {
        return Ok(());
    }
    let manifest = crate::read_json(&manifest_path)?;
    if !crate::artifacts::obfuscated(&manifest) {
        return Ok(());
    }
    remap(root, &lock.json, &manifest, jdk, &jars, log)
}

/// `<side>.jar` (official names) as `<side>-named.jar` beside it, from the version JSON's
/// `<side>_mappings`; done again only when the input or the mappings change.
fn remap(
    root: &Path,
    lock_json: &Value,
    manifest: &Value,
    jdk: &crate::java::Jdk,
    jars: &Path,
    log: crate::Log,
) -> Result<(), String> {
    let pin = lock_json
        .get("tiny_remapper")
        .ok_or("research-lock.json has no tiny_remapper")?;
    let text = |key: &str| {
        pin.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("research-lock.json tiny_remapper has no {key}"))
    };
    let remapper = fetch(
        text("url")?,
        &root.join(format!(
            ".local/tools/tiny-remapper-{}-fat.jar",
            text("version")?
        )),
        text("sha256")?,
        "sha256",
    )?;
    for (side, input) in [("client", "client.jar"), ("server", "server-game.jar")] {
        let download = manifest
            .get("downloads")
            .and_then(|d| d.get(&format!("{side}_mappings")))
            .ok_or_else(|| format!("The version JSON has no {side}_mappings"))?;
        let field = |key: &str| {
            download
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{side}_mappings has no {key}"))
        };
        let mappings = fetch(
            field("url")?,
            &jars.join(format!("{side}-mappings.txt")),
            field("sha1")?,
            "sha1",
        )?;
        let input = jars.join(input);
        let output = jars.join(format!("{side}-named.jar"));
        let stamp = jars.join(format!("{side}-named.sha256"));
        let key = format!(
            "{} {} {}",
            crate::digest(&input, "sha256")?,
            crate::digest(&mappings, "sha256")?,
            text("sha256")?
        );
        if output.exists() && std::fs::read_to_string(&stamp).is_ok_and(|s| s == key) {
            continue;
        }
        log(&format!("Remapping the {side} jar to Mojang's names"));
        let tiny = jars.join(format!("{side}-named.tiny"));
        let proguard = std::fs::read_to_string(&mappings)
            .map_err(|e| format!("{}: {e}", mappings.display()))?;
        std::fs::write(&tiny, proguard_to_tiny(&proguard)?)
            .map_err(|e| format!("{}: {e}", tiny.display()))?;
        let partial = jars.join(format!("{side}-named.partial.jar"));
        let _ = std::fs::remove_file(&partial);
        let log_path = jars.join(format!("{side}-remap.log"));
        let log_file =
            std::fs::File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        let arg = |p: &Path| p.display().to_string();
        jdk.java(
            &[
                "-jar".into(),
                arg(&remapper),
                arg(&input),
                arg(&partial),
                arg(&tiny),
                "official".into(),
                "named".into(),
            ],
            None,
            &log_file,
        )
        .map_err(|e| {
            format!(
                "{e} while remapping the {side} jar; see {}",
                log_path.display()
            )
        })?;
        std::fs::rename(&partial, &output).map_err(|e| format!("{}: {e}", output.display()))?;
        std::fs::write(&stamp, key).map_err(|e| format!("{}: {e}", stamp.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::proguard_to_tiny;

    #[test]
    fn fields_methods_and_arrays_use_official_descriptors() {
        let text = "# comment\nnet.minecraft.Foo -> a:\n    int count -> b\n    1:2:net.minecraft.Foo[] all(int,java.lang.String) -> c\n";
        assert_eq!(
            proguard_to_tiny(text).unwrap(),
            "v1\tofficial\tnamed\nCLASS\ta\tnet/minecraft/Foo\nFIELD\ta\tI\tb\tcount\nMETHOD\ta\t(ILjava/lang/String;)[La;\tc\tall\n"
        );
    }

    #[test]
    fn line_ranges_once_and_inlined_frames_dropped() {
        let text = "net.minecraft.Foo -> a:\n    1:2:void run() -> c\n    3:4:void run() -> c\n    5:6:void net.minecraft.Bar.inlined():7:8 -> c\n";
        assert_eq!(proguard_to_tiny(text).unwrap().matches("METHOD").count(), 1);
    }

    #[test]
    fn unreadable_line_is_refused() {
        assert!(proguard_to_tiny("net.minecraft.Foo -> a:\n    garbage\n").is_err());
    }
}
