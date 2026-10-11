//! Debug aid: scripted input, to exercise the compositor without a human.
//!
//! A script is a list of steps separated by `;`. Each step is a delay in
//! milliseconds since the previous step, then a command:
//!
//! ```text
//! 500 move 780 27; 100 click; 300 key alt+1
//! ```

/// XKB keycodes (evdev code + 8) of the keys a script can name.
const KEYS: [(&str, u32); 10] = [
    ("alt", 64),
    ("ctrl", 37),
    ("shift", 50),
    ("enter", 36),
    ("backspace", 22),
    ("1", 10),
    ("2", 11),
    ("e", 26),
    ("pageup", 112),
    ("pagedown", 117),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Moves the pointer to a position on the output.
    Move {
        x: f64,
        y: f64,
    },
    /// Presses and releases the left mouse button.
    Click,
    Press(u32),
    Release(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedStep {
    /// Milliseconds since the previous step.
    pub after_ms: u64,
    pub step: Step,
}

pub fn parse(script: &str) -> Result<Vec<TimedStep>, String> {
    let mut steps = Vec::new();
    for text in script
        .split(';')
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        let mut words = text.split_whitespace();
        let after_ms = words
            .next()
            .and_then(|delay| delay.parse().ok())
            .ok_or_else(|| format!("`{text}` must start with a delay in milliseconds"))?;
        let command = words.next().unwrap_or_default();
        let arguments: Vec<&str> = words.collect();

        let parsed = match (command, arguments.as_slice()) {
            ("click", []) => vec![Step::Click],
            ("move", [x, y]) => match (x.parse(), y.parse()) {
                (Ok(x), Ok(y)) => vec![Step::Move { x, y }],
                _ => return Err(format!("`{text}`: a position is two numbers")),
            },
            ("key", [chord]) => chord_steps(chord)?,
            _ => return Err(format!("`{text}`: unknown command `{command}`")),
        };
        // Only the first step of a chord waits.
        let delays = std::iter::once(after_ms).chain(std::iter::repeat(0));
        steps.extend(
            delays
                .zip(parsed)
                .map(|(after_ms, step)| TimedStep { after_ms, step }),
        );
    }
    Ok(steps)
}

/// Keys of a chord such as `alt+shift+e`: pressed in order, released in
/// reverse.
fn chord_steps(chord: &str) -> Result<Vec<Step>, String> {
    let keycodes = chord
        .split('+')
        .map(|name| {
            let name = name.to_lowercase();
            KEYS.iter()
                .find(|(known, _)| *known == name)
                .map(|(_, keycode)| *keycode)
                .ok_or_else(|| format!("unknown key `{name}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let presses = keycodes.iter().copied().map(Step::Press);
    let releases = keycodes.iter().rev().copied().map(Step::Release);
    Ok(presses.chain(releases).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(script: &str) -> Vec<(u64, Step)> {
        parse(script)
            .unwrap()
            .into_iter()
            .map(|timed| (timed.after_ms, timed.step))
            .collect()
    }

    #[test]
    fn an_empty_script_has_no_step() {
        assert_eq!(steps(""), []);
        assert_eq!(steps("  ;  "), []);
    }

    #[test]
    fn a_move_takes_a_position() {
        assert_eq!(
            steps("200 move 780 27.5"),
            [(200, Step::Move { x: 780.0, y: 27.5 })]
        );
    }

    #[test]
    fn steps_are_separated_by_semicolons() {
        assert_eq!(
            steps("200 move 10 20; 50 click ;0 click"),
            [
                (200, Step::Move { x: 10.0, y: 20.0 }),
                (50, Step::Click),
                (0, Step::Click),
            ]
        );
    }

    #[test]
    fn a_key_chord_presses_in_order_and_releases_in_reverse() {
        assert_eq!(
            steps("300 key alt+shift+e"),
            [
                (300, Step::Press(64)),
                (0, Step::Press(50)),
                (0, Step::Press(26)),
                (0, Step::Release(26)),
                (0, Step::Release(50)),
                (0, Step::Release(64)),
            ]
        );
    }

    #[test]
    fn key_names_ignore_case() {
        assert_eq!(
            steps("0 key Alt+PageDown"),
            [
                (0, Step::Press(64)),
                (0, Step::Press(117)),
                (0, Step::Release(117)),
                (0, Step::Release(64)),
            ]
        );
    }

    #[test]
    fn enter_and_backspace_can_be_named() {
        assert_eq!(
            steps("0 key ctrl+alt+backspace")[..3],
            [
                (0, Step::Press(37)),
                (0, Step::Press(64)),
                (0, Step::Press(22))
            ]
        );
        assert_eq!(steps("0 key enter")[0], (0, Step::Press(36)));
    }

    #[test]
    fn an_unknown_command_is_reported() {
        let error = parse("10 dance").unwrap_err();

        assert!(error.contains("dance"), "{error}");
    }

    #[test]
    fn an_unknown_key_is_reported() {
        let error = parse("10 key alt+f13").unwrap_err();

        assert!(error.contains("f13"), "{error}");
    }

    #[test]
    fn a_missing_delay_is_reported() {
        assert!(parse("click").is_err());
    }

    #[test]
    fn a_move_without_position_is_reported() {
        assert!(parse("10 move 5").is_err());
    }
}
