extern crate rand;
extern crate rustbox;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;

pub mod freecell;
pub mod freecell_game;
pub mod game;

pub fn run() {
    use freecell_game::FreeCellGame;
    use game::Game;

    let mut game = Game::new("FreeCell").expect("failed to initialize console");
    let mut fc = FreeCellGame::new();

    game.run(&mut fc).unwrap();
}
