//! English spoken punctuation, applied once before publishing a transcript.

/// Whisper context uses spelled-out examples rather than instruction following.
pub(crate) const SPOKEN_PUNCTUATION_PROMPT: &str =
    "Hello comma how are you question mark Next sentence period Full stop Exclamation mark \
     Colon semicolon new line new paragraph literal comma literal question mark";

/// Remove automatic punctuation first so explicit commands survive. Lowercase
/// is a separate, final operation and intentionally changes all English text.
pub fn format_transcription(
    text: &str,
    language: &str,
    spoken: bool,
    remove: bool,
    lowercase: bool,
) -> String {
    if language != "en" {
        return text.to_string();
    }
    let text = if remove {
        remove_auto_punctuation(text)
    } else {
        text.to_string()
    };
    let text = apply_spoken_punctuation(&text, language, spoken);
    if lowercase {
        text.to_lowercase()
    } else {
        text
    }
}

fn punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '“' | '”' | '‘' | '’' | '—' | '–' | '…' | '«' | '»' | '。' | '、' | '！' | '？'
        )
}

/// Paired wrappers and marks following their closing delimiter are external to
/// a value. Strip those together before deciding which suffixes belong to it.
fn unwrapped_value(token: &str) -> &str {
    let mut core = token;
    loop {
        let Some(open) = core.chars().next() else {
            break;
        };
        let close = match open {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            '<' => '>',
            '"' => '"',
            '\'' => '\'',
            '“' => '”',
            '‘' => '’',
            '«' => '»',
            _ => break,
        };
        let body = &core[open.len_utf8()..];
        let mut depth = 1;
        let closing = body.char_indices().find_map(|(index, c)| {
            if c == close {
                depth -= 1;
            } else if c == open {
                depth += 1;
            }
            (depth == 0).then_some(index)
        });
        let Some(end) = closing else { break };
        if !body[end + close.len_utf8()..].chars().all(punctuation) {
            break;
        }
        core = &body[..end];
    }
    core.trim_start_matches(|c| matches!(c, '"' | '\'' | '(' | '[' | '{' | '“' | '‘' | '<' | '«'))
}

fn url_value(core: &str) -> bool {
    let lower = core.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("www.")
}

fn value_core(token: &str) -> &str {
    let mut core = unwrapped_value(token);
    let is_url = url_value(core);
    while let Some(last) = core.chars().next_back() {
        let unbalanced = match last {
            ')' => core.matches(')').count() > core.matches('(').count(),
            ']' => core.matches(']').count() > core.matches('[').count(),
            '>' => true,
            _ => false,
        };
        if unbalanced
            || matches!(last, '.' | ',' | '"' | '\'' | '”' | '’' | '…')
            || !is_url && matches!(last, '!' | '?' | ';' | ':')
        {
            core = &core[..core.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    core
}

fn opaque_value(core: &str) -> bool {
    url_value(core) || core.contains('@') && core.contains('.')
}

fn remove_auto_punctuation(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for chunk in text.split_inclusive(char::is_whitespace) {
        let token = chunk.trim_end_matches(char::is_whitespace);
        let core = value_core(token);
        // URLs and addresses are opaque values; preserve their internal syntax.
        if opaque_value(core) {
            output.push_str(core);
        } else {
            let chars: Vec<char> = token.chars().collect();
            let mut separator = false;
            for (i, &c) in chars.iter().enumerate() {
                let previous = i.checked_sub(1).and_then(|j| chars.get(j)).copied();
                let next = chars.get(i + 1).copied();
                let in_word = previous.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric);
                let in_number = previous.is_some_and(|c| c.is_ascii_digit())
                    && next.is_some_and(|c| c.is_ascii_digit());
                let keep = !punctuation(c)
                    || matches!(c, '\'' | '’' | '-' | '_') && in_word
                    || matches!(c, '.' | ',') && in_number
                    || matches!(c, '-' | '+')
                        && !previous.is_some_and(char::is_alphanumeric)
                        && next.is_some_and(|c| c.is_ascii_digit());
                if keep {
                    if separator
                        && c.is_alphanumeric()
                        && output
                            .chars()
                            .next_back()
                            .is_some_and(char::is_alphanumeric)
                    {
                        output.push(' ');
                    }
                    output.push(c);
                    separator = false;
                } else {
                    // Defer spacing until the entire punctuation run is gone.
                    separator = true;
                }
            }
        }
        output.push_str(&chunk[token.len()..]);
    }
    // Clean gaps left by standalone marks, preserving paragraph boundaries.
    output
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Explicit punctuation replaces adjacent automatic punctuation. Other text is
/// copied unchanged; consecutive punctuation commands use the last mark, while
/// explicit line breaks accumulate. Literal escapes are processed only once.
pub fn apply_spoken_punctuation(text: &str, language: &str, enabled: bool) -> String {
    if !enabled || language != "en" {
        return text.to_string();
    }

    let words = words(text);
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut i = 0;
    let mut space = false;
    let mut capitalize = false;
    let mut consecutive_mark = false;

    while i < words.len() {
        let literal = words[i].text.eq_ignore_ascii_case("literal")
            && words
                .get(i + 1)
                .is_some_and(|next| text[words[i].end..next.start].chars().all(boundary));
        let command_index = i + usize::from(literal);
        let Some((count, mark)) = command(text, &words, command_index) else {
            i += 1;
            continue;
        };
        let mut start = words[i].start;
        let mut end = words[command_index + count - 1].end;
        if !literal {
            // Only horizontal whitespace and sentence punctuation belong to
            // this boundary. Quotes, brackets and existing line breaks survive.
            start -= text[..start].len() - text[..start].trim_end_matches(boundary).len();
            // A URL may legitimately end in any of these boundary characters.
            // Remove marks outside the preceding value, never inside it.
            if let Some(protected_end) = i
                .checked_sub(1)
                .and_then(|previous| words[previous].protected_end)
            {
                start = start.max(protected_end);
            }
            end += text[end..].len() - text[end..].trim_start_matches(boundary).len();
        }
        let gap = &text[cursor..start.max(cursor)];
        append_text(&mut output, gap, &mut space, &mut capitalize);
        if !gap.is_empty() {
            consecutive_mark = false;
        }
        if literal {
            let spelling = words[command_index..command_index + count]
                .iter()
                .map(|word| word.text)
                .collect::<Vec<_>>()
                .join(" ");
            append_text(&mut output, &spelling, &mut space, &mut capitalize);
            consecutive_mark = false;
        } else if mark.starts_with('\n') {
            output.push_str(mark);
            space = false;
            consecutive_mark = false;
        } else {
            if consecutive_mark {
                output.pop();
            }
            output.push_str(mark);
            space = true;
            capitalize = matches!(mark, "." | "?" | "!");
            consecutive_mark = true;
        }
        cursor = end;
        i = command_index + count;
    }
    append_text(&mut output, &text[cursor..], &mut space, &mut capitalize);
    output
}

fn boundary(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '.'
            | ','
            | '?'
            | '!'
            | ':'
            | ';'
            | '…'
            | '—'
            | '–'
            | '。'
            | '、'
            | '！'
            | '？'
    )
}

struct Word<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    protected_end: Option<usize>,
}

fn words(text: &str) -> Vec<Word<'_>> {
    let mut offset = 0;
    text.split_inclusive(char::is_whitespace)
        .filter_map(|chunk| {
            let start = offset;
            offset += chunk.len();
            let token = chunk.trim_end_matches(char::is_whitespace);
            let unwrapped = unwrapped_value(token);
            let value = if url_value(unwrapped) {
                unwrapped
            } else {
                value_core(token)
            };
            let protected_end = if opaque_value(value) {
                Some(start + token.find(value).unwrap() + value.len())
            } else {
                None
            };
            let core = chunk.trim_matches(|c: char| {
                c.is_whitespace()
                    || boundary(c)
                    || matches!(c, '\'' | '"' | '“' | '”' | '(' | ')' | '[' | ']')
            });
            if core.is_empty() {
                return None;
            }
            let start = start + chunk.find(core).unwrap();
            Some(Word {
                text: core,
                start,
                end: start + core.len(),
                protected_end,
            })
        })
        .collect()
}

fn command(text: &str, words: &[Word<'_>], i: usize) -> Option<(usize, &'static str)> {
    let first = words.get(i)?.text;
    for (name, mark) in [
        ("comma", ","),
        ("period", "."),
        ("colon", ":"),
        ("semicolon", ";"),
    ] {
        if first.eq_ignore_ascii_case(name) {
            return Some((1, mark));
        }
    }
    let second = words.get(i + 1)?.text;
    if !text[words[i].end..words[i + 1].start].chars().all(boundary) {
        return None;
    }
    for (a, b, mark) in [
        ("full", "stop", "."),
        ("question", "mark", "?"),
        ("exclamation", "mark", "!"),
        ("new", "line", "\n"),
        ("new", "paragraph", "\n\n"),
    ] {
        if first.eq_ignore_ascii_case(a) && second.eq_ignore_ascii_case(b) {
            return Some((2, mark));
        }
    }
    None
}

fn append_text(output: &mut String, text: &str, space: &mut bool, capitalize: &mut bool) {
    if text.is_empty() {
        return;
    }
    let closing_quote = text.starts_with(['"', '\''])
        && text
            .chars()
            .nth(1)
            .is_none_or(|c| c.is_whitespace() || boundary(c));
    if *space
        && !output.ends_with(char::is_whitespace)
        && !closing_quote
        && !text.starts_with(|c: char| c.is_whitespace() || matches!(c, '”' | ')' | ']'))
    {
        output.push(' ');
    }
    *space = false;
    if *capitalize {
        if text
            .split_whitespace()
            .next()
            .is_some_and(|token| opaque_value(value_core(token)))
        {
            // Case can be meaningful in an address or URL. Lowercase remains
            // an explicit separate setting; sentence capitalization skips it.
            output.push_str(text);
            *capitalize = false;
            return;
        }
        if let Some((index, letter)) = text.char_indices().find(|(_, c)| c.is_alphabetic()) {
            output.push_str(&text[..index]);
            output.extend(letter.to_uppercase());
            output.push_str(&text[index + letter.len_utf8()..]);
            *capitalize = false;
            return;
        }
    }
    output.push_str(text);
}

#[cfg(test)]
mod tests {
    use super::{apply_spoken_punctuation, format_transcription};

    fn process(text: &str) -> String {
        apply_spoken_punctuation(text, "en", true)
    }

    #[test]
    fn converts_all_supported_commands() {
        for (command, mark) in [
            ("comma", ","),
            ("period", "."),
            ("full stop", "."),
            ("question mark", "?"),
            ("exclamation mark", "!"),
            ("colon", ":"),
            ("semicolon", ";"),
        ] {
            assert_eq!(process(&format!("Hello {command}")), format!("Hello{mark}"));
        }
    }

    #[test]
    fn explicit_commands_replace_auto_punctuation_on_both_sides() {
        assert_eq!(
            process("Hello. Comma, how are you? Question mark."),
            "Hello, how are you?"
        );
        assert_eq!(process("Ready! question mark, next"), "Ready? Next");
        assert_eq!(process("First, semicolon. second."), "First; second.");
    }

    #[test]
    fn commands_ignore_case_and_model_punctuation_between_words() {
        assert_eq!(process("ready QUESTION. MARK! next"), "ready? Next");
        assert_eq!(process("one FULL STOP two"), "one. Two");
    }

    #[test]
    fn newlines_replace_auto_sentence_endings() {
        assert_eq!(
            process("First item. New line. Second item."),
            "First item\nSecond item."
        );
        assert_eq!(process("one new paragraph two"), "one\n\ntwo");
        assert_eq!(process("new line hello new paragraph"), "\nhello\n\n");
        assert_eq!(process("new line"), "\n");
    }

    #[test]
    fn literal_escape_preserves_single_and_multiword_commands() {
        assert_eq!(process("Say literal comma."), "Say comma.");
        assert_eq!(process("Say literal question mark."), "Say question mark.");
        assert_eq!(
            process("literal new paragraph then comma continue"),
            "new paragraph then, continue"
        );
        assert_eq!(
            process("Literal Comma and literal colon."),
            "Comma and colon."
        );
        assert_eq!(process("literal hello"), "literal hello");
    }

    #[test]
    fn preserves_non_commands_and_internal_punctuation_exactly() {
        for text in [
            "",
            "  Hello, world!\n",
            "It's 3.14 at https://example.com/comma.",
            "comma-separated semicolons periods colonial",
            "comma@example.com",
            "a/comma /comma comma/file comma.txt foo_comma",
            "new line-item",
        ] {
            assert_eq!(process(text), text);
        }
        assert_eq!(
            process("It's 3.14 comma visit https://example.com/comma."),
            "It's 3.14, visit https://example.com/comma."
        );
    }

    #[test]
    fn disabled_and_other_languages_are_byte_for_byte_unchanged() {
        let text = "  hello comma new line literal period.\n";
        assert_eq!(apply_spoken_punctuation(text, "en", false), text);
        assert_eq!(apply_spoken_punctuation(text, "he", true), text);
    }

    #[test]
    fn capitalizes_only_after_explicit_sentence_marks() {
        assert_eq!(
            process("hello period next sentence comma Alice"),
            "hello. Next sentence, Alice"
        );
        assert_eq!(process("ready question mark it's fine"), "ready? It's fine");
        assert_eq!(process("hello comma World"), "hello, World");
    }

    #[test]
    fn later_consecutive_punctuation_wins_but_line_breaks_accumulate() {
        assert_eq!(process("ready period question mark"), "ready?");
        assert_eq!(process("one new line new line two"), "one\n\ntwo");
        assert_eq!(process("one period new paragraph two"), "one.\n\nTwo");
        assert_eq!(process("comma"), ",");
    }

    #[test]
    fn preserves_quotes_unicode_and_existing_line_breaks() {
        assert_eq!(process("\"hello comma world\""), "\"hello, world\"");
        assert_eq!(process("café comma naïve period über"), "café, naïve. Über");
        assert_eq!(process("first\nsecond comma third"), "first\nsecond, third");
    }

    #[test]
    fn punctuation_before_opening_quotes_has_a_space() {
        assert_eq!(process("hello comma \"world\""), "hello, \"world\"");
        assert_eq!(
            process("hello period 'next sentence'"),
            "hello. 'Next sentence'"
        );
    }

    #[test]
    fn multiword_commands_do_not_cross_quotes_brackets_or_lines() {
        for text in [
            "new (line)",
            "question \"mark\"",
            "new\nparagraph",
            "literal\ncomma",
        ] {
            // A standalone comma on a new line remains a command; it is not escaped.
            let expected = if text == "literal\ncomma" {
                "literal\n,"
            } else {
                text
            };
            assert_eq!(process(text), expected);
        }
    }

    #[test]
    fn removal_strips_sentence_punctuation_and_wrapping_quotes() {
        for (text, expected) in [
            (
                "Hello, World! Ready? Yes; let's go: now.",
                "Hello World Ready Yes let's go now",
            ),
            ("\"Hello\", (World)! [Ready?]", "Hello World Ready"),
            ("Hello , world .", "Hello world"),
            ("Hello—World… Really?!", "Hello World Really"),
            ("“Hello,” she said.", "Hello she said"),
            ("", ""),
            (".!?", ""),
            ("\nHello!\n\nWorld.\n", "\nHello\n\nWorld\n"),
        ] {
            assert_eq!(
                format_transcription(text, "en", false, true, false),
                expected,
                "{text}"
            );
        }
    }

    #[test]
    fn removal_preserves_internal_word_and_numeric_punctuation() {
        for text in [
            "don't",
            "It’s fine",
            "3.14",
            "1,000.25",
            "-3.14",
            "well-known",
            "snake_case",
        ] {
            assert_eq!(format_transcription(text, "en", false, true, false), text);
        }
        assert_eq!(
            format_transcription("It's 3.14, or 1,000.25!", "en", false, true, false),
            "It's 3.14 or 1,000.25"
        );
    }

    #[test]
    fn removal_preserves_urls_and_email_addresses() {
        for text in [
            "https://Example.com/comma?q=One&v=2#top",
            "http://example.com/",
            "www.example.com/path",
            "user.name+tag@example.com",
        ] {
            assert_eq!(format_transcription(text, "en", false, true, false), text);
        }
        assert_eq!(
            format_transcription(
                "Visit https://example.com/test, please.",
                "en",
                false,
                true,
                false
            ),
            "Visit https://example.com/test please"
        );
    }

    #[test]
    fn all_eight_option_combinations_have_explicit_expected_outputs() {
        let text = "Hello, comma World! period Next.";
        for (spoken, remove, lowercase, expected) in [
            (false, false, false, "Hello, comma World! period Next."),
            (false, false, true, "hello, comma world! period next."),
            (false, true, false, "Hello comma World period Next"),
            (false, true, true, "hello comma world period next"),
            (true, false, false, "Hello, World. Next."),
            (true, false, true, "hello, world. next."),
            (true, true, false, "Hello, World. Next"),
            (true, true, true, "hello, world. next"),
        ] {
            assert_eq!(
                format_transcription(text, "en", spoken, remove, lowercase),
                expected,
                "spoken={spoken} remove={remove} lowercase={lowercase}"
            );
        }
    }

    #[test]
    fn removal_keeps_spoken_marks_linebreaks_and_literal_escapes() {
        for (text, expected) in [
            ("Hello, period next.", "Hello. Next"),
            ("Hello! comma world.", "Hello, world"),
            (
                "one. new line two! new paragraph three.",
                "one\ntwo\n\nthree",
            ),
            (
                "Say literal comma, then period next.",
                "Say comma then. Next",
            ),
            ("new paragraph", "\n\n"),
        ] {
            assert_eq!(
                format_transcription(text, "en", true, true, false),
                expected
            );
        }
    }

    #[test]
    fn lowercase_is_independent_and_unicode_aware() {
        assert_eq!(
            format_transcription("HELLO, Über!", "en", false, false, true),
            "hello, über!"
        );
        assert_eq!(
            format_transcription("HELLO period NEXT", "en", true, false, true),
            "hello. next"
        );
    }

    #[test]
    fn removal_does_not_merge_words_separated_by_punctuation_runs() {
        for text in [
            "Hello...World",
            "Hello::World",
            "Hello?!World",
            "Hello—…World",
        ] {
            assert_eq!(
                format_transcription(text, "en", false, true, false),
                "Hello World"
            );
        }
    }

    #[test]
    fn removal_preserves_url_parentheses_schemes_and_query_syntax() {
        for text in [
            "https://example.com/a_(b)",
            "HTTPS://Example.com/a",
            "https://example.com/?",
            "https://example.com/has-comma?q=comma+period",
            "http://[::1]/path",
        ] {
            assert_eq!(format_transcription(text, "en", false, true, false), text);
            assert_eq!(format_transcription(text, "en", true, true, false), text);
        }
        assert_eq!(
            format_transcription("(https://example.com/a_(b)).", "en", false, true, false),
            "https://example.com/a_(b)"
        );
    }

    #[test]
    fn removal_preserves_signed_numbers_inside_wrappers() {
        for (text, expected) in [
            ("(-3.14)", "-3.14"),
            ("[+3.14]", "+3.14"),
            ("-1,000.25!", "-1,000.25"),
        ] {
            assert_eq!(
                format_transcription(text, "en", false, true, false),
                expected
            );
        }
    }

    #[test]
    fn commands_override_unicode_auto_punctuation() {
        assert_eq!(process("Hello… comma world"), "Hello, world");
        assert_eq!(process("Hello！ period next"), "Hello. Next");
    }

    #[test]
    fn removal_keeps_each_explicit_command() {
        for (command, mark) in [
            ("comma", ","),
            ("period", "."),
            ("full stop", "."),
            ("question mark", "?"),
            ("exclamation mark", "!"),
            ("colon", ":"),
            ("semicolon", ";"),
            ("new line", "\n"),
            ("new paragraph", "\n\n"),
        ] {
            let input = format!("Hello, {command}.");
            assert_eq!(
                format_transcription(&input, "en", true, true, false),
                format!("Hello{mark}")
            );
        }
    }

    #[test]
    fn all_options_leave_hebrew_and_unknown_languages_unchanged() {
        for language in ["he", "fr", ""] {
            for spoken in [false, true] {
                for remove in [false, true] {
                    for lowercase in [false, true] {
                        let text = "שלום, HELLO! comma\n";
                        assert_eq!(
                            format_transcription(text, language, spoken, remove, lowercase),
                            text
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn spoken_commands_preserve_meaningful_url_endings_with_removal_on_or_off() {
        for url in [
            "https://example.com/?",
            "https://example.com/path;",
            "https://example.com/!",
            "https://example.com/path:",
            "https://example.com/?q=1;",
            "HTTPS://Example.com/?",
            "www.example.com/path;",
            "https://example.com/a_(b)?",
        ] {
            for remove in [false, true] {
                for (command, mark) in [
                    ("comma", ","),
                    ("period", "."),
                    ("question mark", "?"),
                    ("new line", "\n"),
                ] {
                    assert_eq!(
                        format_transcription(
                            &format!("Visit {url} {command} next"),
                            "en",
                            true,
                            remove,
                            false
                        ),
                        format!(
                            "Visit {url}{mark}{}",
                            if matches!(mark, "." | "?") {
                                " Next"
                            } else if mark == "\n" {
                                "next"
                            } else {
                                " next"
                            }
                        ),
                        "url={url} command={command} remove={remove}"
                    );
                }
            }
        }
    }

    #[test]
    fn spoken_commands_keep_opaque_values_and_only_remove_external_marks() {
        assert_eq!(
            process("Visit https://example.com/?. comma next"),
            "Visit https://example.com/?., next"
        );
        assert_eq!(
            process("Visit https://example.com/? , comma next"),
            "Visit https://example.com/?, next"
        );
        assert_eq!(
            process("Visit https://example.com/? literal comma"),
            "Visit https://example.com/? comma"
        );
        assert_eq!(
            process("Visit (https://example.com/?) comma next"),
            "Visit (https://example.com/?), next"
        );
        assert_eq!(
            process("hello period user@example.com"),
            "hello. user@example.com"
        );
    }

    #[test]
    fn removal_cleans_email_sentence_marks_and_wrappers() {
        for spoken in [false, true] {
            for (input, expected) in [
                ("Contact user@example.com!", "Contact user@example.com"),
                (
                    "Contact user@example.com; then continue.",
                    "Contact user@example.com then continue",
                ),
                (
                    "Contact (user@example.com); next.",
                    "Contact user@example.com next",
                ),
                (
                    "Contact [user@example.com]?! next.",
                    "Contact user@example.com next",
                ),
                (
                    "Contact \"user@example.com\"! next.",
                    "Contact user@example.com next",
                ),
                (
                    "Contact ([user.name+tag@example.com]); next.",
                    "Contact user.name+tag@example.com next",
                ),
            ] {
                assert_eq!(
                    format_transcription(input, "en", spoken, true, false),
                    expected,
                    "spoken={spoken}: {input}"
                );
            }
        }
    }

    #[test]
    fn removal_cleans_external_wrappers_without_shortening_url_values() {
        for spoken in [false, true] {
            for (input, expected) in [
                (
                    "Visit (https://example.com/?); next.",
                    "Visit https://example.com/? next",
                ),
                (
                    "Visit [https://example.com/path;]! next.",
                    "Visit https://example.com/path; next",
                ),
                (
                    "Visit \"https://example.com/!\"; next.",
                    "Visit https://example.com/! next",
                ),
                (
                    "Visit (https://example.com/a_(b);)! next.",
                    "Visit https://example.com/a_(b); next",
                ),
                (
                    "Visit ([https://example.com/?]); next.",
                    "Visit https://example.com/? next",
                ),
            ] {
                assert_eq!(
                    format_transcription(input, "en", spoken, true, false),
                    expected,
                    "spoken={spoken}: {input}"
                );
            }
        }
    }

    #[test]
    fn spoken_commands_override_only_punctuation_outside_addresses_and_wrappers() {
        assert_eq!(
            process("Contact user@example.com! comma next"),
            "Contact user@example.com, next"
        );
        assert_eq!(
            process("Visit (https://example.com/?); comma next"),
            "Visit (https://example.com/?), next"
        );
        assert_eq!(
            process("Visit \"https://example.com/path;\"! comma next"),
            "Visit \"https://example.com/path;\", next"
        );
        assert_eq!(
            format_transcription(
                "Contact (user@example.com); comma next",
                "en",
                true,
                true,
                false
            ),
            "Contact user@example.com, next"
        );
    }
}
