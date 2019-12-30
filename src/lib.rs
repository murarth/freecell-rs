extern crate mortal;
extern crate rand;
#[macro_use] extern crate serde;
extern crate serde_json;
extern crate term_game;

pub mod freecell;
pub mod freecell_game;

pub fn run() {
    use freecell_game::FreeCellGame;
    use term_game::Game;

    let mut game = Game::new("FreeCell").expect("failed to initialize console");
    let mut fc = FreeCellGame::new();

    game.run(&mut fc).unwrap();
}
