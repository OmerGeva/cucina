//! What a project says it can do, read off the files already in its directory.
//!
//! The point is a suggestion, not an import. An auto-added list of twelve npm
//! scripts is noise; the set a person actually runs is two or three. So this
//! offers a short list, the user taps one to run it, and running it is what
//! adds it. Nothing here is ever persisted.

use serde::Serialize;
use std::path::Path;

/// More than this and a suggestion stops being a suggestion.
const MAX: usize = 6;

/// Which manifest a suggestion came from, so the menu can say
/// "FROM YOUR GEMFILE" rather than offering commands from nowhere.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    /// The file the commands were read from, for the menu header. Empty when
    /// there are none.
    pub source: String,
    pub commands: Vec<String>,
}

impl Suggestions {
    fn none() -> Suggestions {
        Suggestions {
            source: String::new(),
            commands: Vec::new(),
        }
    }
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
}

/// The keys of a top-level JSON object field, in the order the file lists
/// them — `package.json` and `composer.json` both keep scripts this way.
fn json_object_keys(text: &str, field: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    value
        .get(field)
        .and_then(|v| v.as_object())
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// Which package manager the lockfile implies. npm is the fallback because a
/// project with no lockfile at all is most often a fresh `npm init`.
fn node_prefix(dir: &Path) -> &'static str {
    if dir.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if dir.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm run"
    }
}

/// The keys of one TOML table, without a TOML parser. Cucina has no TOML
/// dependency and this is the only thing that would want one: a scan for the
/// `[table]` header and the `key =` lines under it is a smaller, more
/// predictable thing to own than a whole grammar.
fn toml_table_keys(text: &str, table: &str) -> Vec<String> {
    let header = format!("[{table}]");
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            let key = key.trim().trim_matches(['"', '\'']).trim();
            if !key.is_empty() {
                out.push(key.to_string());
            }
        }
    }
    out
}

/// Makefile targets: a name at the start of a line followed by a colon.
/// Pattern rules, `.PHONY` and the like start with a dot or contain a `%`, and
/// none of them are things a person runs by name.
fn make_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t', '#', '.']) {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        // `foo := bar` is a variable, not a target.
        if rest.starts_with('=') {
            continue;
        }
        let name = name.trim();
        if name.is_empty() || name.contains(['%', '$', ' ', '=']) {
            continue;
        }
        if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        if !out.contains(&name.to_string()) {
            out.push(name.to_string());
        }
    }
    out
}

/// Rake tasks, read rather than run. `rake -T` would be the accurate answer,
/// but opening a dropdown is not a reason to execute a project's Rakefile —
/// so the declarations are parsed statically and an unusual one is missed
/// rather than a shell being spawned behind the user's back.
fn rake_tasks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("task ") else {
            continue;
        };
        let name: String = rest
            .trim_start_matches([':', '"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// What this directory offers, in the order the handoff table lists them: the
/// first manifest that yields anything wins, so a Rails app suggests Rails
/// commands rather than whatever its package.json happens to hold.
fn found(dir: &Path) -> Suggestions {
    let has = |name: &str| dir.join(name).exists();

    if let Some(text) = read(dir, "package.json") {
        let prefix = node_prefix(dir);
        let commands: Vec<String> = json_object_keys(&text, "scripts")
            .into_iter()
            .map(|s| format!("{prefix} {s}"))
            .collect();
        if !commands.is_empty() {
            return Suggestions {
                source: "package.json".into(),
                commands,
            };
        }
    }

    if let Some(gemfile) = read(dir, "Gemfile") {
        let mut commands = Vec::new();
        if has("bin/rails") {
            commands.extend([
                "bin/rails db:migrate".to_string(),
                "bin/rails db:seed".to_string(),
                "bin/rails console".to_string(),
                "bin/rails db:rollback STEP=1".to_string(),
            ]);
        }
        if gemfile.contains("rspec") {
            commands.push("bundle exec rspec".to_string());
        }
        if !has("bin/rails") {
            if let Some(rakefile) = read(dir, "Rakefile") {
                commands.extend(
                    rake_tasks(&rakefile)
                        .into_iter()
                        .map(|t| format!("bundle exec rake {t}")),
                );
            }
        }
        if !commands.is_empty() {
            return Suggestions {
                source: "Gemfile".into(),
                commands,
            };
        }
    }

    if has("manage.py") {
        return Suggestions {
            source: "manage.py".into(),
            commands: vec![
                "python manage.py migrate".into(),
                "python manage.py shell".into(),
                "python manage.py createsuperuser".into(),
            ],
        };
    }

    if has("alembic.ini") {
        return Suggestions {
            source: "alembic.ini".into(),
            commands: vec!["alembic upgrade head".into(), "alembic downgrade -1".into()],
        };
    }

    if let Some(text) = read(dir, "pyproject.toml") {
        let mut commands = toml_table_keys(&text, "project.scripts");
        if commands.is_empty() {
            commands = toml_table_keys(&text, "tool.poetry.scripts");
        }
        if !commands.is_empty() {
            return Suggestions {
                source: "pyproject.toml".into(),
                commands,
            };
        }
    }

    if let Some(text) = read(dir, "Makefile") {
        let commands: Vec<String> = make_targets(&text)
            .into_iter()
            .map(|t| format!("make {t}"))
            .collect();
        if !commands.is_empty() {
            return Suggestions {
                source: "Makefile".into(),
                commands,
            };
        }
    }

    if has("Cargo.toml") {
        return Suggestions {
            source: "Cargo.toml".into(),
            commands: vec!["cargo test".into(), "cargo check".into()],
        };
    }

    if has("go.mod") {
        return Suggestions {
            source: "go.mod".into(),
            commands: vec!["go test ./...".into()],
        };
    }

    if let Some(text) = read(dir, "composer.json") {
        let commands: Vec<String> = json_object_keys(&text, "scripts")
            .into_iter()
            .map(|s| format!("composer run {s}"))
            .collect();
        if !commands.is_empty() {
            return Suggestions {
                source: "composer.json".into(),
                commands,
            };
        }
    }

    Suggestions::none()
}

/// What to offer for a server, given what it already starts with and what the
/// user has already kept. Read on demand — there is no cache and no watcher,
/// because a handful of small files read when a menu opens is cheaper than the
/// state needed to remember them.
pub fn suggest(dir: &Path, start_command: &str, kept: &[String]) -> Suggestions {
    let mut found = found(dir);
    let start = start_command.trim();
    found.commands.retain(|c| {
        // The start command is already the play button on the card, and
        // offering it here invites a second copy of the server.
        c != start && !kept.iter().any(|k| k == c)
    });
    found.commands.truncate(MAX);
    if found.commands.is_empty() {
        return Suggestions::none();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_npm_scripts_and_picks_the_prefix_from_the_lockfile() {
        let dir = tempdir("npm");
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"vite","seed":"node seed.js"}}"#,
        )
        .unwrap();
        assert_eq!(
            found(&dir).commands,
            vec!["npm run dev", "npm run seed"],
            "no lockfile means npm"
        );

        std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(found(&dir).commands[0], "pnpm dev");
    }

    /// A Rails app has a package.json too, and suggesting `npm run build`
    /// ahead of `db:migrate` would get the order exactly backwards — except
    /// that the table puts package.json first on purpose, so this pins which
    /// behaviour we actually chose.
    #[test]
    fn rails_commands_come_from_the_gemfile() {
        let dir = tempdir("rails");
        std::fs::write(dir.join("Gemfile"), "gem 'rails'\ngem 'rspec-rails'").unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/rails"), "#!/usr/bin/env ruby").unwrap();

        let s = found(&dir);
        assert_eq!(s.source, "Gemfile");
        assert!(s.commands.contains(&"bin/rails db:migrate".to_string()));
        assert!(s.commands.contains(&"bundle exec rspec".to_string()));
    }

    #[test]
    fn reads_makefile_targets_and_skips_what_is_not_one() {
        let targets = make_targets(
            "\
.PHONY: start test\n\
CFLAGS := -O2\n\
start:\n\
\tgo run .\n\
test: start\n\
\tgo test ./...\n\
%.o: %.c\n\
\tcc -c $<\n",
        );
        assert_eq!(targets, vec!["start", "test"]);
    }

    #[test]
    fn reads_one_toml_table_without_bleeding_into_the_next() {
        let text = "\
[project]\n\
name = \"app\"\n\
\n\
[project.scripts]\n\
serve = \"app:main\"\n\
worker = \"app:worker\"\n\
\n\
[tool.ruff]\n\
line-length = 100\n";
        assert_eq!(
            toml_table_keys(text, "project.scripts"),
            vec!["serve", "worker"]
        );
        assert!(toml_table_keys(text, "tool.poetry.scripts").is_empty());
    }

    #[test]
    fn rake_tasks_are_parsed_rather_than_run() {
        let tasks = rake_tasks(
            "desc 'seed'\ntask :seed do\nend\ntask 'db:reset' => :environment do\nend\n",
        );
        assert_eq!(tasks, vec!["seed", "db:reset"]);
    }

    #[test]
    fn drops_the_start_command_and_anything_already_kept() {
        let dir = tempdir("filter");
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"vite","seed":"x","lint":"y"}}"#,
        )
        .unwrap();
        let kept = vec!["npm run lint".to_string()];
        let s = suggest(&dir, "npm run dev", &kept);
        assert_eq!(s.commands, vec!["npm run seed"]);
    }

    #[test]
    fn never_offers_more_than_a_short_list() {
        let dir = tempdir("cap");
        let scripts: Vec<String> = (0..20).map(|i| format!("\"s{i}\":\"x\"")).collect();
        std::fs::write(
            dir.join("package.json"),
            format!("{{\"scripts\":{{{}}}}}", scripts.join(",")),
        )
        .unwrap();
        assert_eq!(suggest(&dir, "", &[]).commands.len(), MAX);
    }

    /// A directory with nothing recognisable must say so rather than naming a
    /// manifest it did not read anything from.
    #[test]
    fn an_unrecognised_project_offers_nothing() {
        let dir = tempdir("bare");
        let s = suggest(&dir, "", &[]);
        assert!(s.commands.is_empty());
        assert!(s.source.is_empty());
    }

    /// Each test gets its own directory under the system temp dir; the suite
    /// runs in parallel and they would otherwise read each other's manifests.
    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cucina-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
