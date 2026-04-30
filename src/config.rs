use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_speed")]
    pub speed: u64,
    #[serde(default = "default_background")]
    pub background: bool,
    #[serde(default = "default_order")]
    pub order: String,
    #[serde(default = "default_loop", rename = "loop", alias = "loop_playback")]
    pub loop_playback: bool,
    #[serde(default = "default_ignore_patterns")]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub speed_rules: Vec<String>,
}

fn default_theme() -> String {
    "tokyo-night".to_string()
}

fn default_speed() -> u64 {
    30
}

fn default_background() -> bool {
    true
}

fn default_order() -> String {
    "random".to_string()
}

fn default_loop() -> bool {
    false
}

fn default_ignore_patterns() -> Vec<String> {
    Vec::new()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            speed: default_speed(),
            background: default_background(),
            order: default_order(),
            loop_playback: default_loop(),
            ignore_patterns: default_ignore_patterns(),
            speed_rules: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let contents = if config_path.exists() {
            // Load existing config and update values to preserve comments
            let existing = fs::read_to_string(&config_path).with_context(|| {
                format!("Failed to read config file: {}", config_path.display())
            })?;

            let mut doc = existing
                .parse::<toml_edit::DocumentMut>()
                .with_context(|| {
                    format!("Failed to parse config file: {}", config_path.display())
                })?;

            // Update values while preserving comments
            doc["theme"] = toml_edit::value(self.theme.as_str());
            doc["speed"] = toml_edit::value(self.speed as i64);
            doc["background"] = toml_edit::value(self.background);
            doc["order"] = toml_edit::value(self.order.as_str());
            doc["loop"] = toml_edit::value(self.loop_playback);
            // Update ignore_patterns as array
            let mut array = toml_edit::Array::new();
            for pattern in &self.ignore_patterns {
                array.push(pattern.as_str());
            }
            doc["ignore_patterns"] = toml_edit::value(array);

            // Update speed_rules as array
            let mut speed_array = toml_edit::Array::new();
            for rule in &self.speed_rules {
                speed_array.push(rule.as_str());
            }
            doc["speed_rules"] = toml_edit::value(speed_array);

            doc.to_string()
        } else {
            // Create new config with comments
            let patterns_str = if self.ignore_patterns.is_empty() {
                "[]".to_string()
            } else {
                let patterns: Vec<String> = self
                    .ignore_patterns
                    .iter()
                    .map(|p| format!("\"{}\"", p))
                    .collect();
                format!("[{}]", patterns.join(", "))
            };

            let speed_rules_str = if self.speed_rules.is_empty() {
                "[]".to_string()
            } else {
                let rules: Vec<String> = self
                    .speed_rules
                    .iter()
                    .map(|r| format!("\"{}\"", r))
                    .collect();
                format!("[{}]", rules.join(", "))
            };

            format!(
                "# gitlogue configuration file\n\
                 # All settings are optional and will use defaults if not specified\n\
                 \n\
                 # Theme to use for syntax highlighting\n\
                 theme = \"{}\"\n\
                 \n\
                 # Typing speed in milliseconds per character\n\
                 speed = {}\n\
                 \n\
                 # Show background colors (set to false for transparent background)\n\
                 background = {}\n\
                 \n\
                 # Commit playback order: random, asc, or desc\n\
                 order = \"{}\"\n\
                 \n\
                 # Loop the animation continuously\n\
                 loop = {}\n\
                 \n\
                 # Ignore patterns (gitignore syntax)\n\
                 # Examples: [\"*.png\", \"*.ipynb\", \"dist/**\"]\n\
                 ignore_patterns = {}\n\
                 \n\
                 # Speed rules for different file types (pattern:milliseconds)\n\
                 # Examples: [\"*.java:50\", \"*.xml:5\", \"*.rs:30\"]\n\
                 speed_rules = {}\n",
                self.theme,
                self.speed,
                self.background,
                self.order,
                self.loop_playback,
                patterns_str,
                speed_rules_str
            )
        };

        fs::write(&config_path, contents)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::home_dir()
            .context("Failed to determine home directory")?
            .join(".config")
            .join("gitlogue");

        fs::create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        Ok(config_dir.join("config.toml"))
    }

    #[allow(dead_code)]
    pub fn themes_dir() -> Result<PathBuf> {
        let config_dir = dirs::home_dir()
            .context("Failed to determine home directory")?
            .join(".config")
            .join("gitlogue")
            .join("themes");

        fs::create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create themes directory: {}",
                config_dir.display()
            )
        })?;

        Ok(config_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    const HOME_VARS: [&str; 4] = ["HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"];

    struct TempHome {
        _lock: MutexGuard<'static, ()>,
        path: PathBuf,
        original_vars: Vec<(&'static str, Option<OsString>)>,
    }

    impl TempHome {
        fn new() -> Result<Self> {
            let lock = env_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let path = env::temp_dir().join(format!(
                "gitlogue-config-tests-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));

            fs::create_dir_all(&path)?;

            let original_vars = HOME_VARS
                .iter()
                .map(|name| (*name, env::var_os(name)))
                .collect();

            env::set_var("HOME", &path);
            env::set_var("USERPROFILE", &path);
            env::remove_var("HOMEDRIVE");
            env::remove_var("HOMEPATH");

            Ok(Self {
                _lock: lock,
                path,
                original_vars,
            })
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            self.original_vars
                .iter()
                .for_each(|(name, value)| match value {
                    Some(value) => env::set_var(name, value),
                    None => env::remove_var(name),
                });

            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_config() -> Config {
        Config {
            theme: "nord".to_string(),
            speed: 12,
            background: false,
            order: "desc".to_string(),
            loop_playback: true,
            ignore_patterns: vec!["dist/**".to_string(), "*.png".to_string()],
            speed_rules: vec!["*.rs:10".to_string(), "*.md:40".to_string()],
        }
    }

    fn assert_config_eq(actual: &Config, expected: &Config) {
        assert_eq!(actual.theme, expected.theme);
        assert_eq!(actual.speed, expected.speed);
        assert_eq!(actual.background, expected.background);
        assert_eq!(actual.order, expected.order);
        assert_eq!(actual.loop_playback, expected.loop_playback);
        assert_eq!(actual.ignore_patterns, expected.ignore_patterns);
        assert_eq!(actual.speed_rules, expected.speed_rules);
    }

    #[test]
    fn load_returns_defaults_when_config_file_is_missing() -> Result<()> {
        let temp_home = TempHome::new()?;
        let config_path = Config::config_path()?;
        let config = Config::load()?;

        assert_eq!(
            config_path,
            temp_home.path.join(".config/gitlogue/config.toml")
        );
        assert!(!config_path.exists());
        assert_eq!(config.theme, "tokyo-night");
        assert_eq!(config.speed, 30);
        assert!(config.background);
        assert_eq!(config.order, "random");
        assert!(!config.loop_playback);
        assert!(config.ignore_patterns.is_empty());
        assert!(config.speed_rules.is_empty());

        Ok(())
    }

    #[test]
    fn load_fills_in_missing_fields_from_defaults() -> Result<()> {
        let _temp_home = TempHome::new()?;
        let config_path = Config::config_path()?;

        fs::write(&config_path, "theme = \"rose-pine\"\n")?;

        let config = Config::load()?;

        assert_eq!(config.theme, "rose-pine");
        assert_eq!(config.speed, 30);
        assert!(config.background);
        assert_eq!(config.order, "random");
        assert!(!config.loop_playback);
        assert!(config.ignore_patterns.is_empty());
        assert!(config.speed_rules.is_empty());

        Ok(())
    }

    #[test]
    fn save_creates_commented_config_and_round_trips_values() -> Result<()> {
        let _temp_home = TempHome::new()?;
        let config = sample_config();
        let config_path = Config::config_path()?;

        config.save()?;

        let contents = fs::read_to_string(&config_path)?;

        assert!(contents.contains("# gitlogue configuration file"));
        assert!(contents.contains("theme = \"nord\""));
        assert!(contents.contains("ignore_patterns = [\"dist/**\", \"*.png\"]"));
        assert!(contents.contains("speed_rules = [\"*.rs:10\", \"*.md:40\"]"));

        let loaded = Config::load()?;
        assert_config_eq(&loaded, &config);

        Ok(())
    }

    #[test]
    fn save_updates_existing_config_without_dropping_comments() -> Result<()> {
        let _temp_home = TempHome::new()?;
        let config_path = Config::config_path()?;
        let config = sample_config();

        fs::write(
            &config_path,
            "# top comment\n\
             theme = \"tokyo-night\"\n\
             # keep this comment\n\
             speed = 30\n\
             background = true\n\
             order = \"random\"\n\
             loop = false\n\
             ignore_patterns = [\"*.tmp\"]\n\
             speed_rules = [\"*.rs:30\"]\n",
        )?;

        config.save()?;

        let contents = fs::read_to_string(&config_path)?;

        assert!(contents.contains("# top comment"));
        assert!(contents.contains("# keep this comment"));
        assert!(contents.contains("theme = \"nord\""));

        let loaded = Config::load()?;
        assert_config_eq(&loaded, &config);

        Ok(())
    }

    #[test]
    fn themes_dir_is_created_under_the_config_home() -> Result<()> {
        let temp_home = TempHome::new()?;
        let themes_dir = Config::themes_dir()?;

        assert_eq!(themes_dir, temp_home.path.join(".config/gitlogue/themes"));
        assert!(themes_dir.is_dir());

        Ok(())
    }
}
