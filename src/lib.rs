//! FreeCell game

pub mod freecell;
pub mod freecell_game;

pub fn run() {
    use freecell_game::FreeCellGame;
    use term_game::Game;

    let mut game = Game::new("FreeCell").expect("failed to initialize console");
    let mut fc = FreeCellGame::new().expect("failed to initialize game");

    game.run(&mut fc).unwrap();
}
