use crate::osmo_auc_gen_fixture;

const FIXTURE: &str =
    include_str!("../compatibility/vectors/osmo-auc-gen/osmo_auc_gen_vectors.ndjson");

#[test]
fn fixture_is_well_formed() {
    osmo_auc_gen_fixture::check_corruptions(FIXTURE);
}

#[cfg(feature = "auth")]
mod with_auth {
    use super::FIXTURE;
    use crate::example;
    use crate::osmo_auc_gen_fixture::{self, Run};

    /// The example's exit status, stdout and stderr.
    pub(crate) fn run_example(args: &[String]) -> (u8, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = example::run(args, &mut out, &mut err).expect("writing to a Vec");
        let text = |bytes| String::from_utf8(bytes).expect("ASCII output");
        (status, text(out), text(err))
    }

    /// Replaces the heading of the algorithm list on stderr, which is the program's own
    /// text, and reports whether stderr has the list. The list itself is the algorithm
    /// names, one per line after two spaces, after an empty line and the heading.
    fn without_heading(stderr: &str) -> (String, bool) {
        let lines: Vec<&str> = stderr.split('\n').collect();
        let Some(first) = lines.iter().position(|line| *line == "  COMP128v1") else {
            return (stderr.to_owned(), false);
        };
        assert!(
            first >= 2 && lines[first - 2].is_empty(),
            "no heading in {stderr:?}"
        );
        let mut lines = lines;
        lines[first - 1] = "<heading>";
        (lines.join("\n"), true)
    }

    /// Compares one run. The banners differ: each is everything before the first empty
    /// line of stdout. Where the program printed help, the help text and the heading of the
    /// algorithm list differ too.
    fn compare(c: &Run) {
        let (status, stdout, stderr) = run_example(&c.args);
        let name = &c.name;
        assert_eq!(status, c.status, "{name}: exit status");
        let (banner, body) = osmo_auc_gen_fixture::split_banner(&stdout)
            .unwrap_or_else(|| panic!("{name}: no banner in {stdout:?}"));
        assert_eq!(format!("{banner}\n"), example::BANNER, "{name}: the banner");
        let (c_stderr, c_help) = without_heading(&c.stderr);
        let (rust_stderr, rust_help) = without_heading(&stderr);
        assert_eq!(rust_help, c_help, "{name}: help");
        assert_eq!(rust_stderr, c_stderr, "{name}: stderr");
        if c_help {
            assert!(!c.body().is_empty(), "{name}: C printed help");
            assert_eq!(body, example::HELP, "{name}: stdout");
        } else {
            assert_eq!(body, c.body(), "{name}: stdout");
        }
    }

    #[test]
    fn the_example_matches_the_c_test_script() {
        let runs = osmo_auc_gen_fixture::validate(FIXTURE);
        let upstream: Vec<_> = runs.iter().filter(|r| r.source == "upstream").collect();
        assert_eq!(upstream.len(), osmo_auc_gen_fixture::SECTIONS[0].1);
        upstream.into_iter().for_each(compare);
    }

    #[test]
    fn the_example_matches_every_harness_case() {
        let runs = osmo_auc_gen_fixture::validate(FIXTURE);
        let harness: Vec<_> = runs.iter().filter(|r| r.source == "harness").collect();
        assert_eq!(harness.len(), osmo_auc_gen_fixture::SECTIONS[1].1);
        harness.into_iter().for_each(compare);
    }
}

#[cfg(feature = "auth")]
pub(crate) use with_auth::run_example;
