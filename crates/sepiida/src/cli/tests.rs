use super::*;

fn apply(args: &[&str]) -> Result<ApplyOptions, CliError> {
    match parse(args)? {
        Command::Apply(options) => Ok(options),
        other => panic!("expected apply, got {other:?}"),
    }
}

#[test]
fn shell_commands_parse() {
    assert_eq!(
        parse(&["shell", "attach"]),
        Ok(Command::Shell(ShellCommand::Attach {
            from: 1,
            remote: false
        }))
    );
    assert_eq!(
        parse(&["shell", "attach", "--from", "3"]),
        Ok(Command::Shell(ShellCommand::Attach {
            from: 3,
            remote: false
        }))
    );
    assert_eq!(
        parse(&["shell", "attach", "--remote", "--from", "2"]),
        Ok(Command::Shell(ShellCommand::Attach {
            from: 2,
            remote: true
        }))
    );
    assert_eq!(
        parse(&["shell", "attach", "--remote", "--remote"]),
        Err(CliError::Repeated("--remote"))
    );
    assert_eq!(
        parse(&["shell", "attach", "--from", "2", "--from", "3"]),
        Err(CliError::Repeated("--from"))
    );
    assert_eq!(
        parse(&["shell", "attach", "--from"]),
        Err(CliError::MissingValue("--from"))
    );
    assert_eq!(
        parse(&["shell", "color", "1a2b3c"]),
        Ok(Command::Shell(ShellCommand::Color(Color::new(
            0x1a, 0x2b, 0x3c
        ))))
    );
    assert_eq!(
        parse(&["shell", "color", "#1A2b3c"]),
        Ok(Command::Shell(ShellCommand::Color(Color::new(
            0x1a, 0x2b, 0x3c
        ))))
    );
    assert_eq!(
        parse(&["shell", "restore"]),
        Ok(Command::Shell(ShellCommand::Restore))
    );
    assert_eq!(
        parse(&["shell", "detach"]),
        Ok(Command::Shell(ShellCommand::Detach))
    );
    for (args, action) in [
        (&["shell", "dump"][..], DumpAction::Start),
        (&["shell", "dump", "collect"], DumpAction::Collect),
        (&["shell", "dump", "stop"], DumpAction::Stop),
    ] {
        assert_eq!(parse(args), Ok(Command::Shell(ShellCommand::Dump(action))));
    }
}

#[test]
fn shell_rejects_bad_arguments() {
    for args in [
        &["shell"][..],
        &["shell", "paint"],
        &["shell", "attach", "now"],
        &["shell", "color"],
        &["shell", "color", "#1a2b3c", "extra"],
        &["shell", "dump", "now"],
        &["shell", "dump", "collect", "extra"],
    ] {
        assert_eq!(parse(args), Err(CliError::ShellArguments), "{args:?}");
    }
    for args in [
        &["shell", "color", "red"][..],
        &["shell", "color", "##123456"],
        &["shell", "attach", "--from", "0"],
        &["shell", "attach", "--from", "x"],
        &["shell", "attach", "--from", "257"],
    ] {
        assert!(
            matches!(parse(args), Err(CliError::InvalidValue(_))),
            "{args:?}"
        );
    }
}

#[test]
fn no_arguments_opens_settings() {
    assert_eq!(parse::<&str>(&[]), Ok(Command::Settings));
}

#[test]
fn help_is_recognised() {
    for flag in ["help", "--help", "-h"] {
        assert_eq!(parse(&[flag]), Ok(Command::Help));
    }
}

#[test]
fn palette_takes_exactly_one_path() {
    assert_eq!(
        parse(&["palette", "a b.png"]),
        Ok(Command::Palette {
            image: "a b.png".to_owned()
        })
    );
    assert_eq!(parse(&["palette"]), Err(CliError::PaletteArguments));
    assert_eq!(
        parse(&["palette", "a", "b"]),
        Err(CliError::PaletteArguments)
    );
}

#[test]
fn apply_without_options_defers_to_the_settings() {
    assert_eq!(apply(&["apply"]), Ok(ApplyOptions::default()));
    assert_eq!(ApplyOptions::default().source, None);
    assert_eq!(ApplyOptions::default().mode, None);
    assert!(!ApplyOptions::default().dry_run);
}

#[test]
fn default_settings_use_auto_and_leave_the_mode_alone() {
    let settings = ApplyOptions::default().settings(&Config::default());
    assert_eq!(
        settings,
        ApplySettings {
            source: SourceChoice::Auto,
            mode: ModeSetting::Unchanged,
            accent: true,
            locked_color: None,
        }
    );
}

#[test]
fn command_line_options_override_the_settings_file() {
    let config = Config {
        source: SourceChoice::WallpaperEngine,
        mode: ModeSetting::Light,
        accent: false,
        locked_color: Some(Color::new(1, 2, 3)),
        ..Config::default()
    };
    let from_file = ApplyOptions::default().settings(&config);
    assert_eq!(from_file.source, SourceChoice::WallpaperEngine);
    assert_eq!(from_file.mode, ModeSetting::Light);
    assert!(!from_file.accent);
    assert_eq!(from_file.locked_color, Some(Color::new(1, 2, 3)));

    let options = ApplyOptions {
        source: Some(SourceChoice::Windows),
        mode: Some(ThemeMode::Dark),
        dry_run: false,
    };
    let overridden = options.settings(&config);
    assert_eq!(overridden.source, SourceChoice::Windows);
    assert_eq!(overridden.mode, ModeSetting::Dark);
}

#[test]
fn apply_reads_all_options_in_any_order() {
    assert_eq!(
        apply(&[
            "apply",
            "--dry-run",
            "--mode",
            "dark",
            "--source",
            "windows"
        ]),
        Ok(ApplyOptions {
            source: Some(SourceChoice::Windows),
            mode: Some(ThemeMode::Dark),
            dry_run: true,
        })
    );
    assert_eq!(
        apply(&["apply", "--source", "wallpaper-engine"]).map(|o| o.source),
        Ok(Some(SourceChoice::WallpaperEngine))
    );
}

#[test]
fn apply_rejects_bad_input() {
    assert_eq!(
        apply(&["apply", "--source"]),
        Err(CliError::MissingValue("--source"))
    );
    assert_eq!(
        apply(&["apply", "--mode"]),
        Err(CliError::MissingValue("--mode"))
    );
    assert!(matches!(
        apply(&["apply", "--source", "kde"]),
        Err(CliError::InvalidValue(_))
    ));
    assert!(matches!(
        apply(&["apply", "--mode", "auto"]),
        Err(CliError::InvalidValue(_))
    ));
    assert_eq!(
        apply(&["apply", "--source", "auto", "--source", "windows"]),
        Err(CliError::Repeated("--source"))
    );
    assert_eq!(
        apply(&["apply", "--verbose"]),
        Err(CliError::UnknownOption {
            command: "apply",
            option: "--verbose".to_owned()
        })
    );
}

#[test]
fn run_takes_no_options() {
    assert_eq!(parse(&["run"]), Ok(Command::Run));
    assert_eq!(
        parse(&["run", "--now"]),
        Err(CliError::UnknownCommand("run".to_owned()))
    );
}

#[test]
fn restore_takes_only_dry_run() {
    assert_eq!(parse(&["restore"]), Ok(Command::Restore { dry_run: false }));
    assert_eq!(
        parse(&["restore", "--dry-run"]),
        Ok(Command::Restore { dry_run: true })
    );
    assert_eq!(
        parse(&["restore", "--dry-run", "--dry-run"]),
        Err(CliError::Repeated("--dry-run"))
    );
    assert!(matches!(
        parse(&["restore", "--dry-run", "--force"]),
        Err(CliError::UnknownOption { .. })
    ));
    assert!(matches!(
        parse(&["restore", "--force"]),
        Err(CliError::UnknownOption { .. })
    ));
}

#[test]
fn unknown_commands_are_rejected() {
    assert_eq!(
        parse(&["frobnicate"]),
        Err(CliError::UnknownCommand("frobnicate".to_owned()))
    );
}
