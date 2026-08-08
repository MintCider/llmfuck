use std::io::{self, IsTerminal, Write};

use anyhow::{Result, bail};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, ClearType},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::model::{Candidate, Risk};

pub fn select(candidates: &[Candidate]) -> Result<Option<&Candidate>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!("candidate selection requires an interactive terminal");
    }
    let mut stderr = io::stderr();
    terminal::enable_raw_mode()?;
    execute!(stderr, cursor::Hide)?;
    let result = run(&mut stderr, candidates);
    let _ = execute!(stderr, cursor::Show, ResetColor);
    let _ = terminal::disable_raw_mode();
    result
}

fn run<'a>(stderr: &mut io::Stderr, candidates: &'a [Candidate]) -> Result<Option<&'a Candidate>> {
    let mut index = 0usize;
    let mut expanded = false;
    let mut armed = false;
    let mut rendered = 0u16;
    loop {
        if rendered > 0 {
            execute!(stderr, cursor::MoveUp(rendered))?;
        }
        rendered = render(stderr, candidates, index, expanded, armed)?;
        stderr.flush()?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                index = index.checked_sub(1).unwrap_or(candidates.len() - 1);
                armed = false;
                expanded = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                index = (index + 1) % candidates.len();
                armed = false;
                expanded = false;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                expanded = !expanded;
                armed = false;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                expanded = false;
                armed = false;
            }
            KeyCode::Char(c @ '1'..='5') => {
                let picked = c as usize - '1' as usize;
                if picked < candidates.len() {
                    index = picked;
                    armed = false;
                }
            }
            KeyCode::Enter => {
                if candidates[index].risk == Risk::High && !armed {
                    armed = true;
                    expanded = true;
                } else {
                    clear(stderr, rendered)?;
                    return Ok(Some(&candidates[index]));
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                clear(stderr, rendered)?;
                return Ok(None);
            }
            _ => {}
        }
    }
}

fn render(
    stderr: &mut io::Stderr,
    candidates: &[Candidate],
    selected: usize,
    expanded: bool,
    armed: bool,
) -> Result<u16> {
    let width = terminal::size()?.0.max(20) as usize;
    let mut lines = 0u16;
    for (i, candidate) in candidates.iter().enumerate() {
        execute!(
            stderr,
            terminal::Clear(ClearType::CurrentLine),
            cursor::MoveToColumn(0)
        )?;
        if i == selected {
            execute!(
                stderr,
                SetAttribute(Attribute::Bold),
                SetForegroundColor(if candidate.risk == Risk::High {
                    Color::Red
                } else {
                    Color::Cyan
                }),
                Print("❯ ")
            )?;
        } else {
            execute!(
                stderr,
                ResetColor,
                SetAttribute(Attribute::Reset),
                Print("  ")
            )?;
        }
        execute!(
            stderr,
            Print(format!("{}. {}", i + 1, candidate.command)),
            ResetColor,
            SetAttribute(Attribute::Reset),
            Print("\r\n")
        )?;
        lines += 1;
        execute!(
            stderr,
            terminal::Clear(ClearType::CurrentLine),
            cursor::MoveToColumn(0),
            SetForegroundColor(Color::DarkGrey),
            Print("     ")
        )?;
        let effect = if i == selected && expanded {
            candidate.effect.clone()
        } else {
            ellipsize(&candidate.effect, width.saturating_sub(5))
        };
        execute!(stderr, Print(effect), ResetColor, Print("\r\n"))?;
        lines += 1;
    }
    execute!(
        stderr,
        terminal::Clear(ClearType::CurrentLine),
        cursor::MoveToColumn(0)
    )?;
    if armed {
        let reason = candidates[selected]
            .risk_reason
            .as_deref()
            .unwrap_or("This command may be destructive or irreversible");
        execute!(
            stderr,
            SetForegroundColor(Color::Red),
            SetAttribute(Attribute::Bold),
            Print(format!(
                "HIGH RISK: {reason}. Press Enter again to execute; Esc cancels."
            )),
            ResetColor,
            SetAttribute(Attribute::Reset)
        )?;
    } else {
        execute!(
            stderr,
            SetForegroundColor(Color::DarkGrey),
            Print("↑/↓ select  → details  Enter run  Esc cancel"),
            ResetColor
        )?;
    }
    execute!(stderr, Print("\r\n"))?;
    Ok(lines + 1)
}

fn clear(stderr: &mut io::Stderr, lines: u16) -> Result<()> {
    if lines > 0 {
        execute!(stderr, cursor::MoveUp(lines))?;
    }
    for n in 0..lines {
        execute!(
            stderr,
            terminal::Clear(ClearType::CurrentLine),
            cursor::MoveToColumn(0)
        )?;
        if n + 1 < lines {
            execute!(stderr, cursor::MoveDown(1))?;
        }
    }
    if lines > 1 {
        execute!(stderr, cursor::MoveUp(lines - 1))?;
    }
    Ok(())
}

fn ellipsize(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_string();
    }
    let target = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > target {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncates_by_display_width() {
        assert_eq!(ellipsize("abcdefgh", 5), "abcd…");
    }
}
