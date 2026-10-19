//! Turns raw chat text into something pleasant to hear.

use std::ops::Range;

const LINK_LABEL: &str = "link";
const MAX_RUN: usize = 3;

#[derive(Debug, Clone, Copy)]
pub struct CleanOpts {
    pub max_chars: usize,
    /// true: speak the emote's name; false: skip emotes silently.
    pub read_emotes: bool,
    /// true: leave URLs as typed; false: say "link" instead.
    pub read_links: bool,
}

impl CleanOpts {
    pub fn plain(max_chars: usize) -> Self {
        Self {
            max_chars,
            read_emotes: false,
            read_links: false,
        }
    }
}

fn is_url(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www.")
}

fn is_noise(c: char) -> bool {
    // Control chars, '@' (mentions keep the name only), variation selectors/ZWJ and emoji blocks.
    c.is_control()
        || c == '@'
        || matches!(c, '\u{200d}' | '\u{fe0f}')
        || matches!(c as u32, 0x2600..=0x27bf | 0x1f000..=0x1faff)
}

/// Handles emotes (char ranges from Twitch) and URLs per `opts`, drops control
/// characters/emoji, caps runs of one repeated character, collapses whitespace and
/// truncates to `opts.max_chars` characters.
pub fn clean(text: &str, emotes: &[Range<usize>], opts: &CleanOpts) -> String {
    let mut buf = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match emotes.iter().find(|r| r.contains(&i)) {
            Some(r) => {
                if opts.read_emotes {
                    let end = r.end.min(chars.len());
                    buf.push(' ');
                    buf.extend(&chars[i..end]);
                    buf.push(' ');
                    i = end.max(i + 1);
                } else {
                    buf.push(' ');
                    i = r.end.max(i + 1);
                }
            }
            None => {
                buf.push(chars[i]);
                i += 1;
            }
        }
    }

    let mut out = String::with_capacity(buf.len());
    for word in buf.split_whitespace() {
        if out.chars().count() >= opts.max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        if !opts.read_links && is_url(word) {
            out.push_str(LINK_LABEL);
            continue;
        }
        let (mut prev, mut run) = (None, 0usize);
        for c in word.chars().filter(|c| !is_noise(*c)) {
            run = if prev == Some(c) { run + 1 } else { 1 };
            prev = Some(c);
            if run <= MAX_RUN {
                out.push(c);
            }
        }
        if out.ends_with(' ') {
            out.pop(); // word was all noise
        }
    }
    if let Some((idx, _)) = out.char_indices().nth(opts.max_chars) {
        out.truncate(idx);
    }
    out.trim_end().to_owned()
}

/// `"{username} says:"` + text. An empty template disables the announcement.
pub fn announce(template: &str, username: &str, text: &str) -> String {
    if template.trim().is_empty() {
        return text.to_owned();
    }
    format!("{} {}", template.replace("{username}", username), text)
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn o(n: usize) -> CleanOpts {
        CleanOpts::plain(n)
    }

    #[test]
    fn replaces_urls_unless_read_links() {
        assert_eq!(
            clean("look https://a.b/c and WWW.x.com now", &[], &o(100)),
            "look link and link now"
        );
        let keep = CleanOpts {
            read_links: true,
            ..o(100)
        };
        assert_eq!(clean("see https://a.b", &[], &keep), "see https://a.b");
    }

    #[test]
    fn emotes_are_skipped_or_read_by_char_range() {
        let em = [0..5, 9..14];
        assert_eq!(clean("Kappa hi Kappa", &em, &o(100)), "hi");
        let read = CleanOpts {
            read_emotes: true,
            ..o(100)
        };
        assert_eq!(clean("Kappa hi Kappa", &em, &read), "Kappa hi Kappa");
    }

    #[test]
    fn emote_ranges_are_char_based_not_byte_based() {
        assert_eq!(clean("привет Kappa", &[7..12], &o(100)), "привет");
    }

    #[test]
    fn caps_repeats_and_collapses_whitespace() {
        assert_eq!(
            clean("  аааааааа   !!!!!!  ok\t\n", &[], &o(100)),
            "ааа !!! ok"
        );
    }

    #[test]
    fn strips_control_mentions_and_emoji() {
        assert_eq!(clean("hi\u{0}\u{7} @bob 😀 x", &[], &o(100)), "hi bob x");
        assert_eq!(clean("😀 😀", &[], &o(100)), "");
    }

    #[test]
    fn truncates_on_char_boundary() {
        assert_eq!(clean("привет мир", &[], &o(6)), "привет");
        assert_eq!(clean("привет мир", &[], &o(8)), "привет м");
        assert!(clean(&"x ".repeat(500), &[], &o(180)).chars().count() <= 180);
    }

    #[test]
    fn out_of_range_emote_indices_do_not_panic() {
        assert_eq!(
            clean(
                "ab",
                &[1..99],
                &CleanOpts {
                    read_emotes: true,
                    ..o(10)
                }
            ),
            "a b"
        );
    }

    #[test]
    fn username_announcement() {
        assert_eq!(announce("{username} says:", "Bob", "hi"), "Bob says: hi");
        assert_eq!(announce("  ", "Bob", "hi"), "hi");
    }
}
