use tower_lsp::lsp_types::Position;

#[derive(Debug)]
pub struct ExpressionSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub enum CompositionElement {
    Frobenius(FrobeniusElement),
    Operation(OperationElement),
}

#[derive(Debug)]
pub struct FrobeniusElement {
    pub start: usize,
    pub end: usize,
    variables: Vec<FrobeniusVariable>,
}

#[derive(Debug)]
struct FrobeniusVariable {
    text: String,
    start: usize,
    end: usize,
    side: FrobeniusSide,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrobeniusSide {
    Source,
    Target,
}

#[derive(Clone, Copy, Debug)]
pub enum PortSide {
    Source,
    Target,
}

#[derive(Debug)]
pub struct FrobeniusOccurrence {
    pub side: FrobeniusSide,
    pub index: usize,
}

#[derive(Debug)]
pub struct OperationElement {
    pub text: String,
    pub start: usize,
}

#[derive(Debug)]
pub struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct OpenDelimiter {
    pub char: char,
    pub offset: usize,
}

impl FrobeniusElement {
    pub fn variable_occurrence(&self, offset: usize, text: &str) -> Option<FrobeniusOccurrence> {
        let variable = self
            .variables
            .iter()
            .find(|variable| {
                variable.text == text && offset >= variable.start && offset < variable.end
            })?;
        Some(FrobeniusOccurrence {
            side: variable.side,
            index: variable.index,
        })
    }
}

pub fn composition_around(text: &str, offset: usize) -> Option<ExpressionSpan> {
    let stack = delimiter_stack_at(text, offset);
    let delimiter = stack.iter().rev().find(|delimiter| delimiter.char == '(')?;
    Some(ExpressionSpan {
        start: delimiter.offset,
        end: matching_close_offset(text, delimiter.offset)?,
    })
}

pub fn scan_composition_elements(
    text: &str,
    expression_start: usize,
    expression_end: usize,
) -> Option<Vec<CompositionElement>> {
    let mut elements = Vec::new();
    let mut offset = expression_start + 1;

    while offset < expression_end {
        offset = skip_whitespace_and_comments(text, offset, expression_end);
        if offset >= expression_end {
            break;
        }

        let ch = char_at(text, offset)?;
        match ch {
            '[' => {
                let end = matching_close_offset(text, offset)?;
                elements.push(CompositionElement::Frobenius(parse_frobenius_element(
                    text, offset, end,
                )?));
                offset = end + 1;
            }
            '(' | '{' => {
                offset = matching_close_offset(text, offset)? + 1;
            }
            _ if is_operation_char(ch) => {
                let token = token_at(text, offset, is_operation_char)?;
                elements.push(CompositionElement::Operation(OperationElement {
                    text: token.text,
                    start: token.start,
                }));
                offset = token.end;
            }
            _ => offset += ch.len_utf8(),
        }
    }

    Some(elements)
}

fn parse_frobenius_element(text: &str, start: usize, end: usize) -> Option<FrobeniusElement> {
    let mut variables = Vec::new();
    let mut side = FrobeniusSide::Source;
    let mut source_index = 0usize;
    let mut target_index = 0usize;
    let mut offset = start + 1;

    while offset < end {
        offset = skip_whitespace_and_comments(text, offset, end);
        if offset >= end {
            break;
        }

        let ch = char_at(text, offset)?;
        if ch == '.' {
            side = FrobeniusSide::Target;
            offset += 1;
            continue;
        }

        if is_variable_char(ch) {
            let token = token_at(text, offset, is_variable_char)?;
            let index = match side {
                FrobeniusSide::Source => {
                    let index = source_index;
                    source_index += 1;
                    index
                }
                FrobeniusSide::Target => {
                    let index = target_index;
                    target_index += 1;
                    index
                }
            };
            variables.push(FrobeniusVariable {
                text: token.text,
                start: token.start,
                end: token.end,
                side,
                index,
            });
            offset = token.end;
            continue;
        }

        offset += ch.len_utf8();
    }

    Some(FrobeniusElement {
        start,
        end,
        variables,
    })
}

fn skip_whitespace_and_comments(text: &str, mut offset: usize, end: usize) -> usize {
    while offset < end {
        let Some(ch) = char_at(text, offset) else {
            break;
        };
        if ch.is_whitespace() {
            offset += ch.len_utf8();
            continue;
        }
        if ch == '#' {
            while offset < end {
                let Some(ch) = char_at(text, offset) else {
                    break;
                };
                offset += ch.len_utf8();
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }
        break;
    }
    offset
}

pub fn token_at(text: &str, offset: usize, predicate: fn(char) -> bool) -> Option<Token> {
    let current =
        char_at(text, offset).or_else(|| offset.checked_sub(1).and_then(|i| char_at(text, i)))?;
    if !predicate(current) {
        return None;
    }

    let mut start = offset.min(text.len());
    while start > 0 {
        let previous = previous_char(text, start)?;
        if !predicate(previous.1) {
            break;
        }
        start = previous.0;
    }

    let mut end = offset.min(text.len());
    if end < text.len() && !text.is_char_boundary(end) {
        end = next_char_boundary(text, end);
    }
    while end < text.len() {
        let Some(ch) = char_at(text, end) else {
            break;
        };
        if !predicate(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    Some(Token {
        text: text[start..end].to_string(),
        start,
        end,
    })
}

pub fn delimiter_stack_at(text: &str, offset: usize) -> Vec<OpenDelimiter> {
    let mut stack = Vec::new();
    let mut in_comment = false;

    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }

        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            continue;
        }

        if ch == '#' {
            in_comment = true;
            continue;
        }

        match ch {
            '(' | '[' | '{' => stack.push(OpenDelimiter {
                char: ch,
                offset: index,
            }),
            ')' | ']' | '}' => {
                let opener = opener_for(ch);
                if let Some(index) = stack.iter().rposition(|delimiter| delimiter.char == opener) {
                    stack.truncate(index);
                }
            }
            _ => {}
        }
    }

    stack
}

pub fn matching_close_offset(text: &str, open_offset: usize) -> Option<usize> {
    let open = char_at(text, open_offset)?;
    let close = close_for(open)?;
    let mut depth = 0usize;
    let mut in_comment = false;

    for (index, ch) in text[open_offset..].char_indices() {
        let absolute = open_offset + index;
        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            continue;
        }

        if ch == '#' {
            in_comment = true;
            continue;
        }

        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(absolute);
            }
        }
    }

    None
}

pub fn offset_at_position(text: &str, position: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut character = 0u32;

    for (offset, ch) in text.char_indices() {
        if line == position.line && character == position.character {
            return Some(offset);
        }
        if ch == '\n' {
            line += 1;
            character = 0;
            continue;
        }
        character += ch.len_utf16() as u32;
    }

    if line == position.line && character == position.character {
        Some(text.len())
    } else {
        None
    }
}

pub fn position_at_offset(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;

    for (current, ch) in text.char_indices() {
        if current >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position { line, character }
}

fn char_at(text: &str, offset: usize) -> Option<char> {
    text.get(offset..)?.chars().next()
}

fn previous_char(text: &str, offset: usize) -> Option<(usize, char)> {
    text.get(..offset)?.char_indices().last()
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    let mut next = offset + 1;
    while next < text.len() && !text.is_char_boundary(next) {
        next += 1;
    }
    next
}

pub fn is_operation_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '-' | '_' | '.' | '*' | '+' | '/' | '|' | '>' | ':' | '=' | '!' | '?'
        )
}

pub fn is_variable_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')
}

fn opener_for(ch: char) -> char {
    match ch {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        _ => ch,
    }
}

fn close_for(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}
