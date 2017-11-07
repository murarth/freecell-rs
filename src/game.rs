use std::io;
use std::time::{Duration, Instant};

use mortal::{Cursor, CursorMode, Event, Key, Screen, Size, Style};

use freecell::Card;

#[allow(unused_variables)]
pub trait GameImpl {
    fn draw(&mut self, game: &mut Game);

    fn on_key_event(&mut self, game: &mut Game, key: Key);

    fn on_tick(&mut self, game: &mut Game) -> io::Result<()> { Ok(()) }
}

pub struct Game {
    screen: Screen,
    title: &'static str,
    game_start: Instant,
    message: Option<Message>,
    pause_time: Option<Instant>,
    pause_duration: Duration,
    redraw: bool,
    loop_level: u32,
}

#[derive(Debug)]
pub struct Message {
    text: String,
    set: Instant,
    duration: Option<Duration>,
}

impl Game {
    /// Creates a new `Game` instance.
    pub fn new(title: &'static str) -> io::Result<Game> {
        let screen = Screen::new(Default::default())?;

        screen.set_cursor_mode(CursorMode::Invisible)?;

        Ok(Game{
            screen,
            title: title,
            game_start: Instant::now(),
            message: None,
            pause_time: None,
            pause_duration: Duration::new(0, 0),
            redraw: true,
            loop_level: 0,
        })
    }

    /// Returns a reference to the active `Screen`.
    pub fn screen(&mut self) -> &mut Screen { &mut self.screen }

    /// Triggers a redraw on the next iteration of the main loop
    pub fn redraw(&mut self) {
        self.redraw = true;
    }

    pub fn quit(&mut self) {
        self.loop_level -= 1;
    }

    /// Main game loop. May be called recursively.
    ///
    /// Call `quit()` to terminate the topmost running loop.
    pub fn run<G: GameImpl>(&mut self, g: &mut G) -> io::Result<()> {
        let level = self.loop_level;
        self.loop_level += 1;

        while self.loop_level > level {
            g.on_tick(self)?;

            if self.redraw {
                self.draw(g)?;
                self.redraw = false;
            }

            if let Some(ev) = self.screen.read_event(Some(Duration::from_millis(100)))? {
                match ev {
                    Event::Key(key) => g.on_key_event(self, key),
                    Event::Resize(..) => self.redraw(),
                    _ => ()
                }
            }

            self.try_expire_message();
        }

        Ok(())
    }

    pub fn draw_title(&mut self, include_time: bool) {
        let Size{columns, ..} = self.screen.size();

        self.screen.set_cursor(Cursor::default());
        self.screen.set_style(Style::REVERSE);

        for _ in 0..columns {
            self.screen.write_char(' ');
        }

        if include_time {
            let s = self.time_str();

            self.screen.set_style(Style::REVERSE);

            self.screen.write_at((0, 1), self.title);

            let col = columns.saturating_sub(6);
            self.screen.write_at((0, col), &s);

            self.screen.clear_attributes();
        }
    }

    pub fn draw_message(&mut self) {
        if let Some(ref msg) = self.message {
            let Size{lines, ..} = self.screen.size();

            self.screen.write_styled_at((lines - 1, 0),
                None, None, Style::BOLD, &msg.text);
        }
    }

    pub fn clear_message(&mut self) {
        self.redraw();
        self.message = None;
    }

    pub fn set_message(&mut self, msg: &str, duration: Option<Duration>) {
        self.redraw();
        self.message = Some(Message{
            text: msg.to_owned(),
            set: Instant::now(),
            duration: duration,
        });
    }

    pub fn paused(&self) -> bool {
        self.pause_time.is_some()
    }

    pub fn pause(&mut self) {
        if self.pause_time.is_none() {
            self.redraw();
            self.pause_time = Some(Instant::now());
        }
    }

    pub fn unpause(&mut self) {
        if let Some(p) = self.pause_time.take() {
            self.redraw();
            self.pause_duration += p.elapsed();
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.paused() {
            self.unpause();
        } else {
            self.pause();
        }
    }

    fn try_expire_message(&mut self) {
        if let Some(Message{set, duration: Some(dur), ..}) = self.message {
            if set.elapsed() >= dur {
                self.message = None;
                self.redraw();
            }
        }
    }

    fn draw<G: GameImpl>(&mut self, g: &mut G) -> io::Result<()> {
        let size = self.screen.size();

        self.screen.clear_screen();

        if size.columns < 50 || size.lines < 20 {
            self.pause();
            self.screen.write_at((0, 0), "screen is too small");
        } else {
            g.draw(self);
        }

        self.refresh()
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.screen.refresh()
    }

    pub fn reset_time(&mut self) {
        self.game_start = Instant::now();
        self.pause_duration = Duration::new(0, 0);
        self.pause_time = None;
    }

    pub fn play_time(&self) -> u32 {
        let dur = match self.pause_time {
            Some(t) => self.game_start.elapsed() - self.pause_duration -
                t.elapsed(),
            None => self.game_start.elapsed() - self.pause_duration
        };

        dur.as_secs() as u32
    }

    fn time_str(&self) -> String {
        time_str(self.play_time())
    }
}

pub fn draw_card(screen: &mut Screen, card: Card, highlight: bool) {
    let sty = if highlight {
        Style::REVERSE
    } else {
        Style::empty()
    };

    let fg = card.suit.color().term_color();
    let bg = None;
    let s = format!("{} {:>2}", card.suit.char(), card.value);

    screen.write_styled(fg, bg, sty, &s);
}

pub fn time_str(secs: u32) -> String {
    format!("{:>2}:{:02}", secs / 60, secs % 60)
}
