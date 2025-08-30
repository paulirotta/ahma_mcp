use ahma_mcp::cli_parser::CliParser;

fn main() {
    let parser = CliParser::new().unwrap();

    // Test all the failing cases from the test
    let test_cases = vec![
        (
            "  -a, --all                  do not ignore entries starting with .",
            Some('a'),
            Some("all"),
            false,
        ),
        (
            "  -h, --human-readable       with -l, print human readable sizes",
            Some('h'),
            Some("human-readable"),
            false,
        ),
        (
            "      --help                 display this help and exit",
            None,
            Some("help"),
            false,
        ),
        (
            "  -C <path>                  run as if git was started in <path>",
            Some('C'),
            None,
            true,
        ),
        (
            "      --config-env=<name>=<envvar>  config environment",
            None,
            Some("config-env"),
            true,
        ),
    ];

    for (line, expected_short, expected_long, expected_takes_value) in test_cases {
        println!("\nTesting line: '{}'", line);
        match parser.parse_option_line(line) {
            Ok(Some(option)) => {
                println!(
                    "  Parsed: short={:?}, long={:?}, desc='{}', takes_value={}",
                    option.short, option.long, option.description, option.takes_value
                );
                println!(
                    "  Expected: short={:?}, long={:?}, takes_value={}",
                    expected_short, expected_long, expected_takes_value
                );

                if option.short != expected_short {
                    println!("  ❌ Short mismatch!");
                }
                if option.long.as_deref() != expected_long.as_deref() {
                    println!("  ❌ Long mismatch!");
                }
                if option.takes_value != expected_takes_value {
                    println!("  ❌ Takes value mismatch!");
                }
            }
            Ok(None) => {
                println!("  ❌ Returned None (expected Some)");
            }
            Err(e) => {
                println!("  ❌ Parse error: {}", e);
            }
        }
    }
}
