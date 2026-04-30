mod animation;
mod config;
mod git;
mod panes;
mod syntax;
mod theme;
mod ui;
mod widgets;

use animation::SpeedRule;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use config::Config;
use git::{DiffMode, GitRepository};
use std::path::{Path, PathBuf};
use theme::Theme;
use ui::UI;

/// Defines the order in which commits are played back during animation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum PlaybackOrder {
    #[default]
    Random,
    Asc,
    Desc,
}

#[derive(Parser, Debug)]
#[command(
    name = "gitlogue",
    version,
    about = "A Git history screensaver - watch your code rewrite itself",
    long_about = "gitlogue is a terminal-based screensaver that replays Git commits as if a ghost developer were typing each change by hand. Characters appear, vanish, and transform with natural pacing and syntax highlighting."
)]
pub struct Args {
    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Path to Git repository (defaults to current directory)"
    )]
    pub path: Option<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "HASH_OR_RANGE",
        help = "Replay a specific commit or commit range (e.g., HEAD~5..HEAD or abc123..)"
    )]
    pub commit: Option<String>,

    #[arg(
        short,
        long,
        value_name = "MS",
        help = "Typing speed in milliseconds per character (overrides config file)"
    )]
    pub speed: Option<u64>,

    #[arg(
        short,
        long,
        value_name = "NAME",
        help = "Theme to use (overrides config file)"
    )]
    pub theme: Option<String>,

    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Show background colors (use --background=false for transparent background, overrides config file)"
    )]
    pub background: Option<bool>,

    #[arg(
        long,
        value_enum,
        value_name = "ORDER",
        help = "Commit playback order (overrides config file)"
    )]
    pub order: Option<PlaybackOrder>,

    #[arg(
        long = "loop",
        num_args = 0..=1,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Loop the animation continuously (useful with --commit for commit ranges)"
    )]
    pub loop_playback: Option<bool>,

    #[arg(long, help = "Display third-party license information")]
    pub license: bool,

    #[arg(
        short = 'a',
        long,
        value_name = "PATTERN",
        value_parser = |s: &str| if s.trim().is_empty() {
            Err("Author pattern cannot be empty".to_string())
        } else {
            Ok(s.to_string())
        },
        help = "Filter commits by author name or email (partial match, case-insensitive)"
    )]
    pub author: Option<String>,

    #[arg(
        long,
        value_name = "DATE",
        help = "Show commits before this date (e.g., '2024-01-01', '1 week ago', 'yesterday')"
    )]
    pub before: Option<String>,

    #[arg(
        long,
        value_name = "DATE",
        help = "Show commits after this date (e.g., '2024-01-01', '1 week ago', 'yesterday')"
    )]
    pub after: Option<String>,

    #[arg(
        short = 'i',
        long = "ignore",
        value_name = "PATTERN",
        action = clap::ArgAction::Append,
        help = "Ignore files matching pattern (gitignore syntax, can be specified multiple times)"
    )]
    pub ignore: Vec<String>,

    #[arg(
        long = "ignore-file",
        value_name = "PATH",
        help = "Path to file containing ignore patterns (one per line, like .gitignore)"
    )]
    pub ignore_file: Option<PathBuf>,

    #[arg(
        long = "speed-rule",
        value_name = "PATTERN:MS",
        action = clap::ArgAction::Append,
        help = "Set typing speed for files matching pattern (e.g., '*.java:50', '*.xml:5'). Can be specified multiple times."
    )]
    pub speed_rule: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Theme management commands
    Theme {
        #[command(subcommand)]
        command: ThemeCommands,
    },
    /// Show staged working tree changes (use --unstaged for unstaged changes)
    Diff {
        #[arg(long, help = "Show unstaged changes instead of staged")]
        unstaged: bool,

        #[arg(
            short,
            long,
            value_name = "MS",
            help = "Typing speed in milliseconds per character"
        )]
        speed: Option<u64>,

        #[arg(short, long, value_name = "NAME", help = "Theme to use")]
        theme: Option<String>,

        #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL",
              help = "Show background colors (use --background=false for transparent)")]
        background: Option<bool>,

        #[arg(long = "loop", num_args = 0..=1, default_missing_value = "true", value_name = "BOOL",
              help = "Loop the animation continuously")]
        loop_playback: Option<bool>,

        #[arg(short = 'i', long = "ignore", value_name = "PATTERN", action = clap::ArgAction::Append,
              help = "Ignore files matching pattern (gitignore syntax)")]
        ignore: Vec<String>,

        #[arg(long = "speed-rule", value_name = "PATTERN:MS", action = clap::ArgAction::Append,
              help = "Set typing speed for files matching pattern (e.g., '*.java:50')")]
        speed_rule: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ThemeCommands {
    /// List all available themes
    List,
    /// Set default theme in config file
    Set {
        #[arg(value_name = "NAME", help = "Theme name to set as default")]
        name: String,
    },
}

impl Args {
    /// Validates the command-line arguments and returns the Git repository path.
    pub fn validate(&self) -> Result<PathBuf> {
        let start_path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));

        if !start_path.exists() {
            anyhow::bail!("Path does not exist: {}", start_path.display());
        }

        let canonical_path = start_path
            .canonicalize()
            .context("Failed to resolve path")?;

        let repo_path = Self::find_git_root(&canonical_path).ok_or_else(|| {
            anyhow::anyhow!(
                "Not a Git repository: {} (or any parent directories)",
                start_path.display()
            )
        })?;

        Ok(repo_path)
    }

    fn find_git_root(start_path: &Path) -> Option<PathBuf> {
        let mut current = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path.to_path_buf()
        };

        loop {
            if current.join(".git").exists() {
                return Some(current);
            }
            if !current.pop() {
                return None;
            }
        }
    }
}

fn is_range_mode(commit: Option<&str>) -> bool {
    commit.is_some_and(|spec| spec.contains(".."))
}

fn resolve_order(
    cli_order: Option<PlaybackOrder>,
    config_order: &str,
    is_range_mode: bool,
    is_filtered: bool,
) -> PlaybackOrder {
    let order = cli_order.unwrap_or(match config_order {
        "asc" => PlaybackOrder::Asc,
        "desc" => PlaybackOrder::Desc,
        _ => PlaybackOrder::Random,
    });

    if (is_range_mode || is_filtered) && cli_order.is_none() {
        PlaybackOrder::Asc
    } else {
        order
    }
}

fn collect_ignore_patterns(
    config_patterns: &[String],
    ignore_file: Option<&Path>,
    cli_patterns: &[String],
) -> Vec<String> {
    let mut patterns = config_patterns.to_vec();

    if let Some(content) = ignore_file.and_then(|path| std::fs::read_to_string(path).ok()) {
        patterns.extend(
            content
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .map(String::from),
        );
    }

    patterns.extend(cli_patterns.iter().cloned());
    patterns
}

fn parse_speed_rules(cli_rules: &[String], config_rules: &[String]) -> Vec<SpeedRule> {
    cli_rules
        .iter()
        .chain(config_rules.iter())
        .filter_map(|rule| {
            SpeedRule::parse(rule).or_else(|| {
                eprintln!("Warning: Invalid speed rule '{}', skipping", rule);
                None
            })
        })
        .collect()
}

fn load_initial_commit(
    repo: &GitRepository,
    commit: Option<&str>,
    order: PlaybackOrder,
) -> Result<git::CommitMetadata> {
    if is_range_mode(commit) {
        repo.set_commit_range(commit.expect("range mode requires a commit spec"))?;
        return match order {
            PlaybackOrder::Random => repo.random_range_commit(),
            PlaybackOrder::Asc => repo.next_range_commit_asc(),
            PlaybackOrder::Desc => repo.next_range_commit_desc(),
        };
    }

    if let Some(commit_hash) = commit {
        return repo.get_commit(commit_hash);
    }

    match order {
        PlaybackOrder::Random => repo.random_commit(),
        PlaybackOrder::Asc => repo.next_asc_commit(),
        PlaybackOrder::Desc => repo.next_desc_commit(),
    }
}

fn should_keep_repo_ref(
    is_range_mode: bool,
    is_filtered: bool,
    is_commit_specified: bool,
    loop_playback: bool,
) -> bool {
    is_range_mode || is_filtered || !is_commit_specified || loop_playback
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --license flag
    if args.license {
        println!("{}", include_str!("../LICENSE-THIRD-PARTY"));
        return Ok(());
    }

    // Handle subcommands
    if let Some(ref command) = args.command {
        match command {
            Commands::Theme { command } => match command {
                ThemeCommands::List => {
                    println!("Available themes:");
                    for theme in Theme::available_themes() {
                        println!("  - {}", theme);
                    }
                    return Ok(());
                }
                ThemeCommands::Set { name } => {
                    // Validate theme exists
                    Theme::load(name)?;

                    // Load existing config or create new one
                    let mut config = Config::load().unwrap_or_default();
                    config.theme = name.clone();
                    config.save()?;

                    let config_path = Config::config_path()?;
                    println!("Theme set to '{}' in {}", name, config_path.display());
                    return Ok(());
                }
            },
            Commands::Diff {
                unstaged,
                speed,
                theme,
                background,
                loop_playback,
                ignore,
                speed_rule,
            } => {
                let repo_path = args.validate()?;
                let repo = GitRepository::open(&repo_path)?;

                let mode = if *unstaged {
                    DiffMode::Unstaged
                } else {
                    DiffMode::Staged
                };

                let metadata = repo.get_working_tree_diff(mode)?;

                if metadata.changes.is_empty() {
                    println!("No changes to display");
                    return Ok(());
                }

                let config = Config::load()?;

                let patterns = collect_ignore_patterns(&config.ignore_patterns, None, ignore);
                git::init_ignore_patterns(&patterns).ok();

                let theme_name = theme.as_deref().unwrap_or(&config.theme);
                let speed = speed.unwrap_or(config.speed);
                let background = background.unwrap_or(config.background);
                let loop_playback = loop_playback.unwrap_or(false);

                let mut theme = Theme::load(theme_name)?;
                if !background {
                    theme = theme.with_transparent_background();
                }

                let speed_rules = parse_speed_rules(speed_rule, &config.speed_rules);

                // Create UI - pass repo ref only if looping (to refresh diff)
                let repo_ref = loop_playback.then_some(&repo);
                let mut ui = UI::new(
                    speed,
                    repo_ref,
                    theme,
                    PlaybackOrder::Asc,
                    loop_playback,
                    None,
                    false,
                    speed_rules,
                );
                ui.set_diff_mode(Some(mode));
                ui.load_commit(metadata);
                ui.run()?;

                return Ok(());
            }
        }
    }

    let repo_path = args.validate()?;
    let mut repo = GitRepository::open(&repo_path)?;

    // Set author filter if specified
    if args.author.is_some() {
        repo.set_author_filter(args.author.clone());
    }

    // Set date filters if specified
    if let Some(ref before_str) = args.before {
        let before_date = git::parse_date(before_str)?;
        repo.set_before_filter(Some(before_date));
    }
    if let Some(ref after_str) = args.after {
        let after_date = git::parse_date(after_str)?;
        repo.set_after_filter(Some(after_date));
    }

    let is_commit_specified = args.commit.is_some();
    let is_range_mode = is_range_mode(args.commit.as_deref());
    let is_filtered = args.author.is_some() || args.before.is_some() || args.after.is_some();

    // Load config: CLI arguments > config file > defaults
    let config = Config::load()?;

    // Initialize ignore patterns: CLI flags > ignore-file > config
    let patterns = collect_ignore_patterns(
        &config.ignore_patterns,
        args.ignore_file.as_deref(),
        &args.ignore,
    );
    git::init_ignore_patterns(&patterns).ok();
    let theme_name = args.theme.as_deref().unwrap_or(&config.theme);
    let speed = args.speed.unwrap_or(config.speed);
    let background = args.background.unwrap_or(config.background);
    let order = resolve_order(args.order, &config.order, is_range_mode, is_filtered);

    let loop_playback = args.loop_playback.unwrap_or(config.loop_playback);
    let mut theme = Theme::load(theme_name)?;

    // Apply transparent background if requested
    if !background {
        theme = theme.with_transparent_background();
    }

    let metadata = load_initial_commit(&repo, args.commit.as_deref(), order)?;

    let speed_rules = parse_speed_rules(&args.speed_rule, &config.speed_rules);

    // Create UI with repository reference
    // Filtered modes (range/author/date) always need repo ref for iteration
    let repo_ref = should_keep_repo_ref(
        is_range_mode,
        is_filtered,
        is_commit_specified,
        loop_playback,
    )
    .then_some(&repo);
    let mut ui = UI::new(
        speed,
        repo_ref,
        theme,
        order,
        loop_playback,
        args.commit.clone(),
        is_range_mode,
        speed_rules,
    );
    ui.load_commit(metadata);
    ui.run()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature, Time};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering as CounterOrdering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestRepo {
        path: PathBuf,
        repo: Repository,
    }

    impl TestRepo {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let unique_id = format!(
                "{}_{}_{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                COUNTER.fetch_add(1, CounterOrdering::SeqCst)
            );
            let path = std::env::temp_dir().join(format!("gitlogue_main_test_{unique_id}"));
            fs::create_dir_all(&path).unwrap();

            let repo = Repository::init(&path).unwrap();
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();

            Self { path, repo }
        }

        fn commit_file(
            &self,
            relative_path: &str,
            content: &str,
            message: &str,
            timestamp: i64,
        ) -> String {
            let file_path = self.path.join(relative_path);
            file_path
                .parent()
                .map(fs::create_dir_all)
                .transpose()
                .unwrap();
            fs::write(&file_path, content).unwrap();

            let mut index = self.repo.index().unwrap();
            index.add_path(Path::new(relative_path)).unwrap();
            index.write().unwrap();

            let tree_id = index.write_tree().unwrap();
            let tree = self.repo.find_tree(tree_id).unwrap();
            let signature =
                Signature::new("Test User", "test@example.com", &Time::new(timestamp, 0)).unwrap();
            let parent = self
                .repo
                .head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok());

            let oid = match parent.as_ref() {
                Some(parent_commit) => self.repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent_commit],
                ),
                None => self
                    .repo
                    .commit(Some("HEAD"), &signature, &signature, message, &tree, &[]),
            }
            .unwrap();

            oid.to_string()
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_args(path: Option<PathBuf>) -> Args {
        Args {
            path,
            commit: None,
            speed: None,
            theme: None,
            background: None,
            order: None,
            loop_playback: None,
            license: false,
            author: None,
            before: None,
            after: None,
            ignore: Vec::new(),
            ignore_file: None,
            speed_rule: Vec::new(),
            command: None,
        }
    }

    #[test]
    fn validate_resolves_git_root_from_nested_files() {
        let test_repo = TestRepo::new();
        let file_path = test_repo.path.join("src/main.rs");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, "fn main() {}\n").unwrap();

        let args = test_args(Some(file_path));

        assert_eq!(args.validate().unwrap(), test_repo.path);
    }

    #[test]
    fn validate_rejects_missing_and_non_repo_paths() {
        let missing = test_args(Some(std::env::temp_dir().join("gitlogue_missing_repo")));
        let missing_error = missing.validate().unwrap_err().to_string();
        assert!(missing_error.contains("Path does not exist"));

        let dir = TestRepo::new();
        let non_repo = dir.path.join("plain-dir");
        fs::create_dir_all(&non_repo).unwrap();
        fs::remove_dir_all(dir.path.join(".git")).unwrap();

        let error = test_args(Some(non_repo))
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains("Not a Git repository"));
    }

    #[test]
    fn resolve_order_defaults_filtered_modes_to_asc_without_overriding_cli() {
        assert_eq!(
            resolve_order(None, "desc", false, false),
            PlaybackOrder::Desc
        );
        assert_eq!(
            resolve_order(None, "random", true, false),
            PlaybackOrder::Asc
        );
        assert_eq!(
            resolve_order(Some(PlaybackOrder::Desc), "asc", true, true),
            PlaybackOrder::Desc
        );
    }

    #[test]
    fn collect_ignore_patterns_merges_config_file_and_cli_sources() {
        let test_repo = TestRepo::new();
        let ignore_file = test_repo.path.join(".gitlogueignore");
        fs::write(&ignore_file, "*.tmp\n\n# comment\ncoverage/**\n").unwrap();

        let patterns = collect_ignore_patterns(
            &["dist/**".to_string()],
            Some(ignore_file.as_path()),
            &["*.png".to_string()],
        );

        assert_eq!(
            patterns,
            vec![
                "dist/**".to_string(),
                "*.tmp".to_string(),
                "coverage/**".to_string(),
                "*.png".to_string()
            ]
        );
    }

    #[test]
    fn parse_speed_rules_keeps_valid_rules_in_priority_order() {
        let rules = parse_speed_rules(
            &["src/**/*.rs:5".to_string(), "invalid".to_string()],
            &["README.md:40".to_string()],
        );

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].speed_ms, 5);
        assert!(rules[0].matches("src/main.rs"));
        assert_eq!(rules[1].speed_ms, 40);
        assert!(rules[1].matches("README.md"));
    }

    #[test]
    fn load_initial_commit_supports_repo_order_hash_and_ranges() {
        let test_repo = TestRepo::new();
        let oldest = test_repo.commit_file("history.txt", "one\n", "first", 1_700_000_000);
        let middle = test_repo.commit_file("history.txt", "two\n", "second", 1_700_000_100);
        let newest = test_repo.commit_file("history.txt", "three\n", "third", 1_700_000_200);
        let repo = GitRepository::open(&test_repo.path).unwrap();

        assert_eq!(
            load_initial_commit(&repo, None, PlaybackOrder::Asc)
                .unwrap()
                .hash,
            oldest
        );
        assert_eq!(
            load_initial_commit(&repo, Some(newest.as_str()), PlaybackOrder::Random)
                .unwrap()
                .hash,
            newest
        );
        assert_eq!(
            load_initial_commit(&repo, Some("HEAD~2..HEAD"), PlaybackOrder::Asc)
                .unwrap()
                .hash,
            middle
        );
        assert_eq!(
            load_initial_commit(&repo, Some("HEAD~2..HEAD"), PlaybackOrder::Desc)
                .unwrap()
                .hash,
            newest
        );
    }

    #[test]
    fn should_keep_repo_ref_only_drops_one_shot_commit_playback() {
        assert!(should_keep_repo_ref(true, false, true, false));
        assert!(should_keep_repo_ref(false, true, true, false));
        assert!(should_keep_repo_ref(false, false, false, false));
        assert!(should_keep_repo_ref(false, false, true, true));
        assert!(!should_keep_repo_ref(false, false, true, false));
    }
}
