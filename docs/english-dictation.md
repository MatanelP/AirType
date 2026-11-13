# English dictation formatting

Settings → **English dictation** has two independent options, both off by default.
They work with shared RunPod, custom RunPod, and OpenAI. Hebrew is unchanged.

| Option | Behavior |
| --- | --- |
| Spoken punctuation | Spoken commands replace automatic punctuation at that position. Other automatic punctuation stays. |
| Remove automatic punctuation | Removes sentence punctuation and wrapping quotes/brackets, and conservatively cleans capitalization after removed sentence endings. Preserves contractions, internal hyphens/underscores, decimals, grouped/signed numbers, HTTP(S)/www URLs, and email addresses. Explicit spoken commands still win when enabled. |

Say **comma**, **dot**, **period** or **full stop**, **question mark**, **exclamation mark**,
**colon**, **semicolon**, **new line**, or **new paragraph**. Commands ignore case.
Say **literal** before a command to write its words: “literal question mark” →
`question mark`. With spoken punctuation enabled, ordinary uses of these command
words also need the literal escape.

For example, a model transcript of `Hello, period next.` becomes `Hello. Next.`
with spoken punctuation enabled. Enable removal too and it becomes `Hello. Next`:
the explicit period remains, while the final automatic period is removed.
Sentence-ending commands capitalize the next word.
Replacing a sentence ending with a comma preserves existing capitalization, so
names remain intact. Every spoken command appends its mark, including repeated
or mixed punctuation: “question mark question mark question mark” writes `???`,
and “question mark exclamation mark” writes `?!`. Automatic punctuation adjacent
to a command is still removed; explicitly spoken marks are never deduplicated.
Consecutive line-break commands accumulate. Leading/trailing spoken line breaks
are preserved. Removal cleans spacing left by deleted marks and preserves
paragraph boundaries.

Removal keeps the opening capital and lowercases clear starters such as “How,”
“The,” and “You” after a removed automatic period, question mark, or exclamation
mark: `Hi. How are you?` → `Hi how are you`. It does not infer a sentence break
from a comma, colon, semicolon, abbreviation, decimal, or protected URL/email.
Paragraph openers, `I` and its contractions, acronyms, and unrecognized capitalized
words stay unchanged. This conservative rule protects ambiguous names and can
leave some unnecessary capitals; it is not a grammar or name-recognition model.
Explicit spoken sentence endings still capitalize the following word.

There is no separate lowercase switch. Older settings containing
`english_lowercase` still load; that obsolete preference is ignored without
altering other settings or credentials.

Spoken commands protect the preceding URL or email token, including meaningful
trailing syntax such as `?`, `;`, `!`, and `:`. For example,
`https://example.com/? comma next` becomes `https://example.com/?, next` with
automatic-punctuation removal either on or off. Sentence capitalization also
preserves URL and email spelling.

Removal treats attached trailing periods, commas, and ellipses as sentence marks.
Those characters remain ambiguous when they are intentionally part of a URL.
With removal off, spoken commands preserve the complete preceding URL token.

Email sentence marks are external punctuation: with removal enabled,
`Contact (user@example.com); next.` becomes `Contact user@example.com next`.
Paired wrappers and marks after their closing delimiter are removed together.
URL suffixes inside those wrappers stay intact: `(https://example.com/?);`
becomes `https://example.com/?`. These rules apply with spoken commands on or off.

The same final text is used for the preview, history, copying, and insertion.
Formatting happens locally after transcription; it adds no model call. Provider
prompts supply command examples, but speech recognition can still omit or
mishear commands. The formatter cannot recover words absent from the transcript.

## Validation

Run `cargo test --manifest-path src-tauri/Cargo.toml --lib transcription::` for
parser and provider-context tests, and
`cargo test --manifest-path src-tauri/Cargo.toml --lib settings::tests` for
defaults, legacy loading, and all four settings combinations.

Parser tests cover all commands, automatic punctuation conflicts, literal
escapes, capitalization, line breaks, Unicode, every option combination,
punctuation runs, contractions, numbers, URLs, emails, and non-English no-ops.
Provider tests verify command context is added only when enabled and Hebrew
prompts stay unchanged. Request context uses Whisper's existing
[prompt support](https://developers.openai.com/api/docs/guides/speech-to-text#prompting).
Live recognition accuracy still needs representative recordings on each provider.

The compact help tooltip supports hover, keyboard focus, click/touch, and Escape.
It chooses above/below placement to fit the visible settings area; long tooltips
scroll when necessary. With focus on the help button, Arrow/PageUp/PageDown keys
scroll clipped text; Home/End reach its beginning/end without moving focus or the
outer panel. Switches retain their full 44×24 size while labels wrap.
`npm run test:settings-ui` checks these behaviors against a running local frontend
at 350px/420px widths and 400px/800px heights using installed Chrome, without
exercising native IPC.

Formatting changes reach clients through signed versioned releases. Platforms
publish progressively into the same release: an update becomes discoverable on
your platform once its signed package is ready. Other platforms can arrive later;
older clients may report a temporary update-check error while their target is
absent. See [release behavior and recovery](../README.md#releasing-maintainers).
