mod app;
mod dbus;
mod input;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, EventStream};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use futures_lite::StreamExt;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::AppAction;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal first so we can show "waiting" in the TUI
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
        original_hook(info);
    }));

    let result = run_with_reconnect(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_with_reconnect(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut events = EventStream::new();

    loop {
        // Try connecting to daemon
        let connect_result = try_connect_and_run(terminal, &mut events).await;

        match connect_result {
            Ok(()) => return Ok(()), // clean quit
            Err(_) => {
                // Show waiting screen, poll for quit or reconnect
                loop {
                    terminal.draw(|f| {
                        let area = f.area();
                        let text = Paragraph::new("Waiting for mixctl daemon...")
                            .style(Style::default().fg(Color::DarkGray))
                            .alignment(Alignment::Center);
                        let centered = Rect::new(0, area.height / 2, area.width, 1);
                        f.render_widget(text, centered);
                    })?;

                    // Check for quit key or wait before retry
                    let timeout = tokio::time::sleep(Duration::from_secs(2));
                    tokio::pin!(timeout);

                    tokio::select! {
                        maybe_event = events.next() => {
                            if let Some(Ok(event::Event::Key(key))) = maybe_event {
                                if matches!(key.code, event::KeyCode::Char('q'))
                                    || (key.code == event::KeyCode::Char('c')
                                        && key.modifiers.contains(event::KeyModifiers::CONTROL))
                                {
                                    return Ok(());
                                }
                            }
                        }
                        _ = &mut timeout => break, // retry connection
                    }
                }
            }
        }
    }
}

async fn try_connect_and_run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    events: &mut EventStream,
) -> Result<()> {
    let (proxy, mut state) = dbus::connect_and_load().await?;
    let mut signal_rx = dbus::subscribe_signals(&proxy).await?;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(3));
    ping_interval.tick().await; // skip first immediate tick

    loop {
        terminal.draw(|f| ui::render(f, &state))?;

        tokio::select! {
            maybe_event = events.next() => {
                if let Some(Ok(event::Event::Key(key))) = maybe_event {
                    if let Some(action) = input::handle_key(key, &state) {
                        match action {
                            AppAction::Quit => return Ok(()),
                            other => {
                                state.handle_action(other, &proxy).await;
                            }
                        }
                    }
                }
            }
            maybe_signal = signal_rx.recv() => {
                match maybe_signal {
                    Some(signal) => state.handle_signal(signal),
                    None => return Err(anyhow::anyhow!("daemon disconnected")),
                }
            }
            _ = ping_interval.tick() => {
                // Periodic liveness check
                if proxy.ping().await.is_err() {
                    return Err(anyhow::anyhow!("daemon disconnected"));
                }
            }
        }
    }
}
