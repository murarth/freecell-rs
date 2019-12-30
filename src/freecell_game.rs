use std::cmp::{max, min};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::replace;
use std::path::PathBuf;
use std::time::Duration;

use dirs::config_dir;
use mortal::{Cursor, Key, Screen, Size, Style};
use serde::{Deserialize, Serialize};
use serde_json as json;

use term_game::{Game, GameImpl};

use crate::freecell::{Card, Color, Face, FreeCell, ACE, JACK, QUEEN, KING};

const SLOT_NAMES: [char; 8] = ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K'];

const HELP_TEXT: &'static str = "\
?             Show this help screen
Q             Quit the game (requires confirmation)
N             Start a new game
P             Pause or unpause the game
S             Show game stats

L             Start card lookup (Esc or Space to end)
R or B        Search for a Red or Black card
0-9 J Q K A   Search for a card value (0 means 10)
L again       Search for lowest cards in play

Esc or Space  Cancel an action
U             Undo an action
Ctrl-R        Redo an action
A-K           Reference a slot on the tableau
R, then A-F   Reference a slot on the reserve
T             Reference the foundation

To move a card, reference the source slot,
  then the destination slot.
Pressing tableau key twice moves to reserve.
";

fn one_sec() -> Option<Duration> { Some(Duration::new(1, 0)) }

pub struct FreeCellGame {
    fc: FreeCell,
    stats: Stats,
    undo: Vec<FreeCell>,
    /// Index into `undo` containing the current state;
    /// equal to `undo.len()` when the current state is new
    undo_index: usize,
    action: Option<Action>,
    locate: Option<Locate>,
    pause_draw: Draw,
    wait_confirm: bool,
    confirm_result: bool,
    try_sweep: bool,
    game_won: bool,
}

#[derive(Deserialize)]
struct StatsFile {
    games: Option<u32>,
    won: Option<u32>,

    highest_time: Option<u32>,
    lowest_time: Option<u32>,
    total_time: Option<u32>,

    longest_streak: Option<u32>,
    current_streak: Option<u32>,
}

#[derive(Default, Serialize)]
struct Stats {
    games: u32,
    won: u32,

    highest_time: u32,
    lowest_time: u32,
    total_time: u32,

    longest_streak: u32,
    current_streak: u32,
}

impl From<StatsFile> for Stats {
    fn from(s: StatsFile) -> Stats {
        Stats{
            games: s.games.unwrap_or(0),
            won: s.won.unwrap_or(0),
            highest_time: s.highest_time.unwrap_or(0),
            lowest_time: s.lowest_time.unwrap_or(0),
            total_time: s.total_time.unwrap_or(0),
            longest_streak: s.longest_streak.unwrap_or(0),
            current_streak: s.current_streak.unwrap_or(0),
        }
    }
}

impl Stats {
    fn win_rate(&self) -> u32 {
        if self.games == 0 {
            0
        } else {
            self.won * 100 / self.games
        }
    }

    fn average_time(&self) -> u32 {
        if self.won == 0 {
            0
        } else {
            self.total_time / self.won
        }
    }
}

fn stats_path() -> PathBuf {
    let config = config_dir().expect("cannot find config dir");
    config.join("mur-freecell/stats.cfg")
}

fn load_stats() -> io::Result<Stats> {
    let mut f = match File::open(&stats_path()) {
        Ok(f) => f,
        Err(ref e) if e.kind() == io::ErrorKind::NotFound =>
            return Ok(Stats::default()),
        Err(e) => return Err(e)
    };

    let mut buf = String::new();

    f.read_to_string(&mut buf)?;

    let sf: StatsFile = json::from_str(&buf)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    Ok(sf.into())
}

fn save_stats(stats: &Stats) -> io::Result<()> {
    let mut f = File::create(&stats_path())?;
    let mut data = json::to_string(stats)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    data.push('\n');

    f.write_all(data.as_bytes())?;

    Ok(())
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Action {
    Foundation,
    Reserve,
    ReserveSlot(u8),
    Slot(u8),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Draw {
    Help,
    Stats,
    Pause,
    Victory,
}

#[derive(Copy, Clone, Debug)]
struct Locate {
    color: Option<Color>,
    what: Match,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Match {
    Nothing,
    Low,
    Value(u8),
}

impl FreeCellGame {
    pub fn new() -> FreeCellGame {
        let stats = match load_stats() {
            Ok(stats) => stats,
            Err(e) => panic!("failed to load stats: {}", e)
        };

        FreeCellGame{
            fc: FreeCell::new(),
            stats: stats,
            undo: Vec::with_capacity(64),
            undo_index: 0,
            action: None,
            locate: None,
            pause_draw: Draw::Pause,
            wait_confirm: false,
            confirm_result: false,
            try_sweep: true,
            game_won: false,
        }
    }

    fn confirm(&mut self, game: &mut Game, msg: &str) -> bool {
        self.wait_confirm = true;
        game.set_message(&format!("{} (y/n)", msg), None);
        game.run(self).unwrap();
        game.clear_message();

        self.wait_confirm = false;
        self.confirm_result
    }

    fn confirm_new_game(&mut self, game: &mut Game) {
        if self.confirm(game, "Start a new game?") {
            self.new_game(game);
        }
    }

    fn confirm_quit(&mut self, game: &mut Game) {
        if self.confirm(game, "Quit game?") {
            self.game_end(game);
            game.quit();
        }
    }

    fn game_end(&mut self, game: &mut Game) {
        if !self.undo.is_empty() {
            self.stats.games += 1;

            if self.game_won {
                self.stats.won += 1;

                let t = game.play_time();

                if self.stats.lowest_time == 0 {
                    self.stats.lowest_time = t;
                } else {
                    self.stats.lowest_time = min(t, self.stats.lowest_time);
                }
                self.stats.highest_time = max(t, self.stats.highest_time);
                self.stats.total_time += t;

                self.stats.current_streak += 1;
                self.stats.longest_streak = max(
                    self.stats.current_streak, self.stats.longest_streak);
            } else {
                self.stats.current_streak = 0;
            }

            self.save_stats(game);
        }
    }

    fn clear_stats(&mut self, game: &mut Game) {
        self.stats = Stats::default();
        self.save_stats(game);
    }

    fn save_stats(&mut self, game: &mut Game) {
        if let Err(e) = save_stats(&self.stats) {
            game.set_message(&format!("Failed to save stats: {}", e), None);
        }
    }

    fn draw_game(&mut self, game: &mut Game) {
        self.draw_field(game);
    }

    fn draw_action(&mut self, game: &mut Game) {
        let s = self.action_str();
        self.draw_status(game, &s);
    }

    fn draw_locate(&mut self, game: &mut Game) {
        if let Some(loc) = self.locate {
            let mut s = "".to_owned();

            s.push_str("L");

            match loc.color {
                Some(Color::Black) => s.push_str(" B"),
                Some(Color::Red) => s.push_str(" R"),
                None => s.push_str(" *")
            }

            match loc.what {
                Match::Nothing => s.push_str(" ?"),
                Match::Low => s.push_str(" LO"),
                Match::Value(n) => {
                    use std::fmt::Write;
                    write!(s, " {}", Face(n)).unwrap()
                }
            }

            self.draw_status(game, &s);
        }
    }

    fn highlight_card(&self, card: Card) -> bool {
        self.locate.map_or(false, |loc| {
            let match_color = loc.color.map_or(true,
                |c| card.suit.color() == c);
            let match_what = match loc.what {
                Match::Nothing => false,
                Match::Low => self.fc.can_move_to_foundation(card),
                Match::Value(n) => card.value.0 == n
            };

            match_color && match_what
        })
    }

    fn highlight_foundation(&self, top: Card) -> bool {
        self.locate.map_or(false, |loc| {
            let match_color = loc.color.map_or(true,
                |c| top.suit.color() == c);
            let match_what = match loc.what {
                Match::Nothing => false,
                Match::Low => false,
                Match::Value(n) => n <= top.value.0
            };

            match_color && match_what
        })
    }

    fn draw_status(&mut self, game: &mut Game, s: &str) {
        let screen = game.screen();
        let Size{lines, columns} = screen.size();
        let n = s.len();

        screen.set_cursor(Cursor{
            column: columns - n - 1,
            line: lines - 1,
        });
        screen.write_styled(None, None, Style::BOLD, s);
    }

    fn draw_field(&mut self, game: &mut Game) {
        let screen = game.screen();
        let Size{columns, ..} = screen.size();

        let startx = (columns - ((4 * 5 + 5) * 2 + 1)) / 2;
        //                       |   |   |    |   ` Plus separator
        //                       |   |   |    ` On each side
        //                       |   |   ` Plus surrounding [] and key
        //                       |   ` Five chars wide (including space in between)
        //                       ` Four cards

        screen.set_cursor(Cursor{
            line: 2,
            column: startx,
        });

        screen.write_str("R [ ");

        for r in self.fc.reserve_slots() {
            match *r {
                Some(c) => draw_card(screen, c, self.highlight_card(c)),
                None => screen.write_str("____")
            }
            screen.write_str(" ");
        }

        screen.write_str("] [ ");

        for f in self.fc.foundation_slots() {
            match *f {
                Some(c) => draw_card(screen, c, self.highlight_foundation(c)),
                None => screen.write_str("____")
            }
            screen.write_str(" ");
        }

        screen.write_str("] T");

        let startx = (columns - (8 * 6)) / 2;
        //                       |   ` Six chars wide (including two spaces between)
        //                       ` Eight slots

        screen.set_cursor(Cursor{
            column: startx,
            line: 4,
        });
        screen.write_styled(None, None, Style::UNDERLINE,
            " A     S     D     F     G     H     J     K  ");

        let max = self.fc.tableau_slots().iter().map(|t| t.len()).max().unwrap();
        let mut cols = self.fc.tableau_slots().iter()
            .map(|t| t.iter()).collect::<Vec<_>>();

        for i in 0..max {
            screen.set_cursor(Cursor{
                column: startx,
                line: i + 5,
            });

            for t in &mut cols {
                match t.next() {
                    Some(&c) => draw_card(screen, c, self.highlight_card(c)),
                    None => screen.write_str("    ")
                }
                screen.write_str("  ");
            }
        }
    }

    fn draw_pause(&mut self, game: &mut Game) {
        match self.pause_draw {
            Draw::Pause => {
                let screen = game.screen();
                let Size{lines, columns} = screen.size();
                let mid = lines / 2;
                let center = columns / 2;
                let col = center.saturating_sub(3);

                screen.write_at((mid, col), "Paused");
            }
            Draw::Help => self.draw_help(game),
            Draw::Stats => self.draw_stats(game),
            Draw::Victory => self.draw_victory(game),
        }
    }

    fn draw_help(&mut self, game: &mut Game) {
        let screen = game.screen();
        let Size{lines, columns} = screen.size();

        let n_lines = HELP_TEXT.lines().count();
        let max_w = HELP_TEXT.lines().map(|l| l.len()).max().unwrap();

        screen.set_cursor(Cursor{
            line: lines.saturating_sub(n_lines).saturating_sub(2) / 2,
            column: columns.saturating_sub(4) / 2,
        });
        screen.write_styled(None, None, Style::BOLD, "HELP");

        let startx = columns.saturating_sub(max_w) / 2;

        // Skip a full line
        screen.next_line(startx);

        for line in HELP_TEXT.lines() {
            screen.next_line(startx);
            screen.write_str(line);
        }
    }

    fn draw_stats(&mut self, game: &mut Game) {
        let screen = game.screen();
        let Size{lines, columns} = screen.size();
        let n_lines = 7;

        let startx = columns.saturating_sub(20) / 2;
        let starty = lines.saturating_sub(n_lines) / 2 - 3;

        screen.set_cursor(Cursor{
            column: columns.saturating_sub(5) / 2,
            line: starty,
        });
        screen.write_styled(None, None, Style::BOLD, "STATS");

        // Skip a full line
        screen.next_line(startx);

        screen.next_line(startx);
        screen.write_str(&format!("Games played:   {:>5}", self.stats.games));
        screen.next_line(startx);
        screen.write_str(&format!("Games won:      {:>5}", self.stats.won));
        screen.next_line(startx);
        screen.write_str(&format!("Win rate:       {:>4}%", self.stats.win_rate()));

        // Skip a line
        screen.next_line(startx);

        screen.next_line(startx);
        screen.write_str(&format!("Longest streak: {:>5}", self.stats.longest_streak));
        screen.next_line(startx);
        screen.write_str(&format!("Current streak: {:>5}", self.stats.current_streak));

        // Skip a line
        screen.next_line(startx);

        screen.next_line(startx);
        screen.write_str(&format!("Average time:   {:>5}",
            time_str(self.stats.average_time())));
        screen.next_line(startx);
        screen.write_str(&format!("Lowest time:    {:>5}",
            time_str(self.stats.lowest_time)));
        screen.next_line(startx);
        screen.write_str(&format!("Highest time:   {:>5}",
            time_str(self.stats.highest_time)));

        // Skip a line
        screen.next_line(startx);

        screen.next_line(startx);
        screen.write_str("Press 'c' to clear");
    }

    fn draw_victory(&mut self, game: &mut Game) {
        let screen = game.screen();
        let Size{lines, columns} = screen.size();

        screen.set_cursor(Cursor{
            column: (columns / 2).saturating_sub(4),
            line: lines / 2,
        });
        screen.write_styled(None, None, Style::BOLD, "You won!");
    }

    fn action(&mut self, game: &mut Game, action: Action) {
        use self::Action::*;

        game.redraw();

        let old = match self.action.take() {
            Some(act) => act,
            None => {
                match action {
                    Foundation => game.set_message("Invalid action", one_sec()),
                    Slot(n) if self.fc.tableau(n as usize).is_empty() => {
                        game.set_message("Tableau slot is empty", one_sec());
                    }
                    _ => self.action = Some(action)
                }
                return;
            }
        };

        match (old, action) {
            (Reserve, Slot(n @ 0 ..= 3)) => {
                if self.fc.reserve(n as usize).is_some() {
                    self.action = Some(Action::ReserveSlot(n));
                } else {
                    game.set_message("Reserve slot is empty", one_sec());
                }
            }
            (Reserve, Slot(_)) => {
                game.set_message("Invalid reserve slot", one_sec())
            }
            (ReserveSlot(n), Foundation) => {
                if let Some(c) = self.fc.reserve(n as usize) {
                    if self.fc.can_move_to_foundation(c) {
                        self.push_undo();
                        self.fc.remove_reserve(n as usize);
                        self.fc.add_to_foundation(c);
                    } else {
                        game.set_message("Cannot move to foundation", one_sec());
                    }
                } else {
                    game.set_message("Reserve slot is empty", one_sec())
                }
            }
            (ReserveSlot(a), Slot(b)) => {
                if let Some(c) = self.fc.reserve(a as usize) {
                    if self.fc.can_move_to_tableau(c, b as usize) {
                        self.push_undo();
                        self.fc.remove_reserve(a as usize);
                        self.fc.add_to_tableau(c, b as usize);
                    } else {
                        game.set_message("Cannot move to tableau", one_sec());
                    }
                } else {
                    game.set_message("Reserve slot is empty", one_sec());
                }
            }
            (Slot(a), Foundation) => {
                match self.fc.tableau(a as usize).last() {
                    Some(&c) => {
                        if self.fc.can_move_to_foundation(c) {
                            self.push_undo();
                            self.fc.pop_tableau(a as usize);
                            self.fc.add_to_foundation(c);
                        } else {
                            game.set_message("Cannot move to foundation", one_sec());
                        }
                    }
                    None => game.set_message("Tableau slot is empty", one_sec())
                }
            }
            (Slot(a), Reserve) => {
                self.move_to_reserve(game, a as usize);
            }
            (Slot(a), Slot(b)) if a == b => {
                self.move_to_reserve(game, a as usize);
            }
            (Slot(a), Slot(b)) => {
                if self.fc.tableau(a as usize).is_empty() {
                    game.set_message("Tableau slot is empty", one_sec());
                } else {
                    self.move_tableau(game, a as usize, b as usize);
                }
            }
            _ => {
                game.set_message("Invalid action", one_sec());
            }
        }

        self.try_sweep = true;
    }

    fn move_to_reserve(&mut self, game: &mut Game, a: usize) {
        if self.fc.tableau(a as usize).is_empty() {
            game.set_message("Tableau slot is empty", one_sec());
        } else {
            if self.fc.reserve_free() {
                self.push_undo();
                let c = self.fc.pop_tableau(a as usize);
                self.fc.add_to_reserve(c);
            } else {
                game.set_message("No free reserve slots", one_sec());
            }
        }
    }

    fn move_tableau(&mut self, game: &mut Game, a: usize, b: usize) {
        match self.fc.tableau(b).last().cloned() {
            Some(top) => {
                let mut mov = None;

                {
                    let tab_a = self.fc.tableau(a);
                    let n = tab_a.len();
                    let size = self.fc.group_size(a);
                    let cap = self.fc.move_capacity(a, b);

                    for i in 1..size + 1 {
                        let c = tab_a[n - i];
                        if c.can_top(top) {
                            if i > cap {
                                game.set_message("Not enough reserve slots to move", one_sec());
                                return;
                            } else {
                                mov = Some((a, b, i));
                                break;
                            }
                        }
                    }
                }

                if let Some((a, b, i)) = mov {
                    self.push_undo();
                    self.fc.move_tableau_group(a, b, i);
                } else {
                    game.set_message("Cannot move cards", one_sec());
                }
            }
            None => {
                self.push_undo();
                let cap = self.fc.move_capacity(a, b);
                self.fc.move_tableau_group(a, b, cap);
            }
        }
    }

    fn sweep_step(&mut self, game: &mut Game) {
        if self.fc.sweep_step(3) {
            game.redraw();
        } else {
            self.try_sweep = false;
        }
    }

    fn action_str(&self) -> String {
        use self::Action::*;

        match self.action {
            Some(Reserve) => "R".to_owned(),
            Some(ReserveSlot(n)) => format!("R {}", SLOT_NAMES[n as usize]),
            Some(Slot(n)) => format!("{}", SLOT_NAMES[n as usize]),
            _ => "".to_owned(),
        }
    }

    fn begin_locate(&mut self) {
        self.locate = Some(Locate{
            color: None,
            what: Match::Nothing,
        });
    }

    fn clear_action(&mut self, game: &mut Game) {
        self.action = None;
        game.redraw();
    }

    fn game_won(&mut self, game: &mut Game) {
        self.game_won = true;
        game.pause();
        self.pause_draw = Draw::Victory;
    }

    fn new_game(&mut self, game: &mut Game) {
        self.game_end(game);
        game.reset_time();

        self.action = None;
        self.locate = None;
        self.game_won = false;
        self.undo.clear();
        self.undo_index = 0;
        self.pause_draw = Draw::Pause;
        self.fc = FreeCell::new();
        self.try_sweep = true;
        game.redraw();
    }

    fn push_undo(&mut self) {
        self.undo.drain(self.undo_index..);
        self.undo.push(self.fc.clone());
        self.undo_index = self.undo.len();
    }

    fn undo(&mut self, game: &mut Game) {
        if self.undo.is_empty() {
            game.set_message("No changes made", one_sec());
        } else if self.undo_index == 0 {
            game.set_message("Already at initial state", one_sec());
        } else {
            let new_fc = self.undo[self.undo_index - 1].clone();

            if self.undo_index == self.undo.len() {
                let fc = replace(&mut self.fc, new_fc);
                self.undo.push(fc);
            } else {
                self.fc = new_fc;
            }
            self.undo_index -= 1;
        }
    }

    fn redo(&mut self, game: &mut Game) {
        if self.undo.is_empty() {
            game.set_message("No changes made", one_sec());
        } else if self.undo_index == self.undo.len() {
            game.set_message("Already at newest state", one_sec());
        } else if self.undo_index == self.undo.len() - 2 {
            self.undo_index += 1;
            self.fc = self.undo.pop().unwrap();
        } else {
            self.undo_index += 1;
            self.fc = self.undo[self.undo_index].clone();

            game.redraw();
            self.try_sweep = true;
        }
    }
}

impl GameImpl for FreeCellGame {
    fn draw(&mut self, game: &mut Game) {
        game.draw_title(true);

        if game.paused() {
            self.draw_pause(game);
        } else {
            self.draw_game(game);
            if self.locate.is_some() {
                self.draw_locate(game);
            } else {
                self.draw_action(game);
            }
        }

        game.draw_message();
    }

    fn on_key_event(&mut self, game: &mut Game, key: Key) {
        if self.wait_confirm {
            match key {
                Key::Char('y') => self.confirm_result = true,
                _ => self.confirm_result = false
            }

            // Terminate this level of the main loop.
            game.quit();
        } else if game.paused() {
            match key {
                Key::Escape | Key::Char(' ') | Key::Char('p')
                        if self.pause_draw != Draw::Victory => {
                    game.toggle_pause()
                }
                Key::Char('c') if self.pause_draw == Draw::Stats => {
                    if self.confirm(game, "Clear stats?") {
                        self.clear_stats(game);
                    }
                }
                Key::Char('n') if self.pause_draw == Draw::Victory =>
                    self.new_game(game),
                Key::Char('n') => self.confirm_new_game(game),
                Key::Char('q') => self.confirm_quit(game),
                _ => return
            }
        } else if self.locate.is_some() {
            match key {
                Key::Escape | Key::Char(' ') => {
                    self.locate = None;
                    game.redraw();
                    return;
                }
                _ => ()
            }

            let loc = self.locate.as_mut().unwrap();

            match key {
                Key::Char('b') => loc.color = Some(Color::Black),
                Key::Char('r') => loc.color = Some(Color::Red),
                Key::Char('l') => loc.what = Match::Low,
                Key::Char('a') => loc.what = Match::Value(ACE),
                Key::Char(n @ '2' ..= '9') => loc.what = Match::Value(n as u8 - b'0'),
                Key::Char('0') => loc.what = Match::Value(10),
                Key::Char('j') => loc.what = Match::Value(JACK),
                Key::Char('q') => loc.what = Match::Value(QUEEN),
                Key::Char('k') => loc.what = Match::Value(KING),
                _ => return
            }
        } else {
            if self.action.is_none() {
                match key {
                    Key::Char('l') => self.begin_locate(),
                    Key::Char('n') => self.confirm_new_game(game),
                    Key::Char('p') => {
                        game.pause();
                        self.pause_draw = Draw::Pause;
                    }
                    Key::Char('q') => self.confirm_quit(game),
                    Key::Char('u') => self.undo(game),
                    Key::Ctrl('r') => self.redo(game),
                    Key::Char('S') => {
                        game.pause();
                        self.pause_draw = Draw::Stats;
                    }
                    Key::Char('?') => {
                        game.pause();
                        self.pause_draw = Draw::Help;
                    }
                    _ => ()
                }
            }

            match key {
                Key::Escape | Key::Char(' ') => self.clear_action(game),
                Key::Char('r') => self.action(game, Action::Reserve),
                Key::Char('t') => self.action(game, Action::Foundation),
                Key::Char('a') => self.action(game, Action::Slot(0)),
                Key::Char('s') => self.action(game, Action::Slot(1)),
                Key::Char('d') => self.action(game, Action::Slot(2)),
                Key::Char('f') => self.action(game, Action::Slot(3)),
                Key::Char('g') => self.action(game, Action::Slot(4)),
                Key::Char('h') => self.action(game, Action::Slot(5)),
                Key::Char('j') => self.action(game, Action::Slot(6)),
                Key::Char('k') => self.action(game, Action::Slot(7)),

                _ => ()
            }
        }

        game.redraw();
    }

    fn on_tick(&mut self, game: &mut Game) -> io::Result<()> {
        if !game.paused() {
            // Redraw the clock
            game.draw_title(true);
            game.refresh()?;

            if self.fc.game_over() {
                self.game_won(game);
            } else if self.try_sweep {
                self.sweep_step(game);
            }
        }

        Ok(())
    }
}

fn draw_card(screen: &mut Screen, card: Card, highlight: bool) {
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

fn time_str(secs: u32) -> String {
    format!("{:>2}:{:02}", secs / 60, secs % 60)
}
