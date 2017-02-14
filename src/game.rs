use std::time::{Duration, Instant};
use std::ops;

use rustbox::{self, Color, Event, EventError,
    InitError, InitOptions, InputMode, Key, RustBox, Style};

use freecell::Card;

pub struct RbWriter<'a> {
    rb: &'a RustBox,
    x: usize,
    y: usize,
}

impl<'a> RbWriter<'a> {
    pub fn new(rb: &RustBox) -> RbWriter {
        RbWriter{
            rb: rb,
            x: 0,
            y: 0,
        }
    }

    pub fn move_to(&mut self, x: usize, y: usize) {
        self.x = x;
        self.y = y;
    }

    pub fn next_line(&mut self, x: usize) {
        self.x = x;
        self.y += 1;
    }

    pub fn write(&mut self, sty: Style, fg: Color, bg: Color, s: &str) {
        let n = s.chars().count();

        self.rb.print(self.x, self.y, sty, fg, bg, s);
        self.x += n;
    }

    pub fn write_card(&mut self, card: Card, highlight: bool) {
        let sty = if highlight {
            rustbox::RB_REVERSE
        } else {
            Style::empty()
        };
        let fg = card.suit.color().console_color();
        let bg = Color::Default;
        let s = format!("{} {:>2}", card.suit.char(), card.value);

        self.write(sty, fg, bg, &s);
    }

    pub fn write_def(&mut self, s: &str) {
        let sty = Style::empty();
        let fg = Color::Default;
        let bg = Color::Default;

        self.write(sty, fg, bg, s);
    }

    pub fn write_sty(&mut self, sty: Style, s: &str) {
        let fg = Color::Default;
        let bg = Color::Default;

        self.write(sty, fg, bg, s);
    }
}

impl<'a> ops::Deref for RbWriter<'a> {
    type Target = RustBox;

    fn deref(&self) -> &RustBox { self.rb }
}

#[allow(unused_variables)]
pub trait GameImpl {
    fn draw(&mut self, game: &mut Game);

    fn on_key_event(&mut self, game: &mut Game, key: Key);

    fn on_tick(&mut self, game: &mut Game) {}
}

pub struct Game {
    rb: RustBox,
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
    pub fn new(title: &'static str) -> Result<Game, InitError> {
        let rb = try!(RustBox::init(InitOptions{
            input_mode: InputMode::Esc,
            buffer_stderr: true,
            .. InitOptions::default()
        }));

        Ok(Game{
            rb: rb,
            title: title,
            game_start: Instant::now(),
            message: None,
            pause_time: None,
            pause_duration: Duration::new(0, 0),
            redraw: true,
            loop_level: 0,
        })
    }

    /// Returns a reference to the active `RustBox`.
    pub fn rb(&self) -> &RustBox { &self.rb }

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
    pub fn run<G: GameImpl>(&mut self, g: &mut G) -> Result<(), EventError> {
        let level = self.loop_level;
        self.loop_level += 1;

        while self.loop_level > level {
            g.on_tick(self);

            if self.redraw {
                self.redraw = false;
                self.draw(g);
            }

            let ev = try!(self.rb.peek_event(Duration::from_millis(100), false));

            match ev {
                Event::KeyEvent(key) => g.on_key_event(self, key),
                Event::ResizeEvent(..) => self.redraw(),
                _ => ()
            }

            self.try_expire_message();
        }

        Ok(())
    }

    pub fn draw_title(&self, include_time: bool) {
        let w = self.rb.width();

        let sty = rustbox::RB_REVERSE;
        let fg = Color::Default;
        let bg = Color::Default;

        for x in 0..w {
            self.rb.print_char(x, 0, sty, fg, bg, ' ');
        }

        if include_time {
            let s = self.time_str();

            self.rb.print(1, 0, sty, fg, bg, self.title);
            self.rb.print(w.saturating_sub(6), 0, sty, fg, bg, &s);
        }
    }

    pub fn draw_message(&self) {
        if let Some(ref msg) = self.message {
            let h = self.rb.height();
            let sty = rustbox::RB_BOLD;
            let fg = Color::Default;
            let bg = Color::Default;

            self.rb.print(0, h - 1, sty, fg, bg, &msg.text);
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
        let expire = self.message.as_ref().map_or(false,
            |msg| msg.duration.map_or(false, |dur| msg.set.elapsed() >= dur));

        if expire {
            self.message = None;
            self.redraw();
        }
    }

    fn draw<G: GameImpl>(&mut self, g: &mut G) {
        self.rb.clear();

        g.draw(self);

        self.refresh();
    }

    pub fn refresh(&self) {
        // Hide the cursor
        self.rb.set_cursor(!0, !0);
        self.rb.present();
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

pub fn time_str(secs: u32) -> String {
    format!("{:>2}:{:02}", secs / 60, secs % 60)
}
