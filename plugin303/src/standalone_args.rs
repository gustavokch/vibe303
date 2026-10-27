//! Command-line preparation for the `standalone` binary.

/// Appends `--sample-rate <rate>` to `args` when the command line does not set the sample
/// rate. `detect` receives the original arguments and runs only in that case.
pub fn with_detected_sample_rate(
    mut args: Vec<String>,
    detect: impl FnOnce(&[String]) -> Option<u32>,
) -> Vec<String> {
    if !sets_sample_rate(&args) {
        if let Some(rate) = detect(&args) {
            args.push(String::from("--sample-rate"));
            args.push(rate.to_string());
        }
    }
    args
}

/// The `--output-device` value, in `--output-device NAME` or `--output-device=NAME` form.
pub fn output_device(args: &[String]) -> Option<&str> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--output-device" {
            return iter.next().map(String::as_str);
        }
        if let Some(name) = arg.strip_prefix("--output-device=") {
            return Some(name);
        }
    }
    None
}

// Every short flag of nih-plug's `WrapperConfig` takes a value, so clap parses any token that
// starts with `-r` as the sample rate: `-r 48000`, `-r48000`, `-r=48000`.
fn sets_sample_rate(args: &[String]) -> bool {
    args.iter().skip(1).any(|arg| {
        arg.starts_with("-r") || arg == "--sample-rate" || arg.starts_with("--sample-rate=")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().copied().map(String::from).collect()
    }

    #[test]
    fn appends_detected_rate_when_unset() {
        let out = with_detected_sample_rate(argv(&["standalone"]), |_| Some(44_100));
        assert_eq!(out, argv(&["standalone", "--sample-rate", "44100"]));
    }

    #[test]
    fn keeps_args_when_detection_fails() {
        let input = argv(&["standalone", "--output-device", "Speakers"]);
        assert_eq!(with_detected_sample_rate(input.clone(), |_| None), input);
    }

    #[test]
    fn explicit_rate_skips_detection() {
        for args in [
            &["standalone", "-r", "48000"][..],
            &["standalone", "-r48000"],
            &["standalone", "-r=48000"],
            &["standalone", "--sample-rate", "96000"],
            &["standalone", "--sample-rate=96000"],
        ] {
            let input = argv(args);
            let out = with_detected_sample_rate(input.clone(), |_| {
                panic!("detection ran despite explicit rate: {args:?}")
            });
            assert_eq!(out, input);
        }
    }

    #[test]
    fn parses_output_device() {
        assert_eq!(
            output_device(&argv(&["standalone", "--output-device", "A B"])),
            Some("A B")
        );
        assert_eq!(
            output_device(&argv(&["standalone", "--output-device=A B"])),
            Some("A B")
        );
        assert_eq!(
            output_device(&argv(&["standalone", "--output-device"])),
            None
        );
        assert_eq!(output_device(&argv(&["standalone", "-r", "44100"])), None);
    }
}
