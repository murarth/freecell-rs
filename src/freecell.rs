use std::cmp::min;
use std::fmt;

use mortal::Color as TermColor;
use rand::{thread_rng, Rng};

pub const ACE: u8 = 1;
pub const JACK: u8 = 11;
pub const QUEEN: u8 = 12;
pub const KING: u8 = 13;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Card {
    pub suit: Suit,
    pub value: Face,
}

impl Card {
    pub fn new(suit: Suit, value: Face) -> Card {
        Card{
            suit: suit,
            value: value,
        }
    }

    /// Returns whether `self` is the same suit as and a lower value than the
    /// given card.
    ///
    /// This is most useful for checking whether `self` has been moved to
    /// foundation: `self.is_lower(foundation) == self is on foundation`.
    pub fn is_lower(&self, other: Card) -> bool {
        self.suit == other.suit && self.value.0 < other.value.0
    }

    /// Returns whether `self` may be placed atop `other` on the tableau.
    pub fn can_top(&self, other: Card) -> bool {
        self.value.0 == other.value.0 - 1 && self.suit.color() != other.suit.color()
    }

    /// Returns whether `self` may succeed the given card; or an empty slot
    /// if the given card is `None`.
    pub fn can_succeed(&self, other: Option<Card>) -> bool {
        match other {
            Some(c) => self.value.0 == c.value.0 + 1,
            None => self.value.0 == ACE
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Face(pub u8);

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            ACE => f.pad("A"),
            JACK => f.pad("J"),
            QUEEN => f.pad("Q"),
            KING => f.pad("K"),
            n => f.pad(&n.to_string())
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Suit {
    Club,
    Diamond,
    Heart,
    Spade,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Color {
    Black,
    Red,
}

impl Color {
    pub fn term_color(&self) -> Option<TermColor> {
        match *self {
            Color::Black => None,
            Color::Red => Some(TermColor::Red),
        }
    }
}

pub const NUM_SUITS: usize = 4;
pub const NUM_FACES: usize = 13;

pub const SUITS: [Suit; NUM_SUITS] = [Suit::Club, Suit::Diamond, Suit::Heart, Suit::Spade];
pub const FACES: [u8; NUM_FACES] = [ACE, 2, 3, 4, 5, 6, 7, 8, 9, 10, JACK, QUEEN, KING];

pub const RESERVE_SLOTS: usize = 4;
pub const FOUNDATION_SLOTS: usize = NUM_SUITS;
pub const TABLEAU_SLOTS: usize = 8;

impl Suit {
    pub fn as_index(&self) -> usize {
        match *self {
            Suit::Club => 0,
            Suit::Diamond => 1,
            Suit::Heart => 2,
            Suit::Spade => 3,
        }
    }

    pub fn color(&self) -> Color {
        match *self {
            Suit::Club | Suit::Spade => Color::Black,
            Suit::Heart | Suit::Diamond => Color::Red,
        }
    }

    pub fn char(&self) -> char {
        match *self {
            Suit::Club => '\u{2663}',
            Suit::Diamond => '\u{2666}',
            Suit::Heart => '\u{2665}',
            Suit::Spade => '\u{2660}',
        }
    }
}

/// Returns a new shuffled deck.
fn new_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);

    for &suit in &SUITS {
        for &value in &FACES {
            deck.push(Card::new(suit, Face(value)));
        }
    }

    thread_rng().shuffle(&mut deck);

    deck
}

fn fill_tableau(deck: Vec<Card>) -> Vec<Vec<Card>> {
    let mut tbl = vec![Vec::new(); TABLEAU_SLOTS];

    for (i, card) in deck.into_iter().enumerate() {
        tbl[i % TABLEAU_SLOTS].push(card);
    }

    tbl
}

#[derive(Clone, Debug)]
pub struct FreeCell {
    reserve: [Option<Card>; RESERVE_SLOTS],
    foundation: [Option<Card>; FOUNDATION_SLOTS],
    tableau: Vec<Vec<Card>>,
}

impl FreeCell {
    pub fn new() -> FreeCell {
        FreeCell{
            reserve: [None; RESERVE_SLOTS],
            foundation: [None; FOUNDATION_SLOTS],
            tableau: fill_tableau(new_deck()),
        }
    }

    pub fn can_move_to_tableau(&self, card: Card, pos: usize) -> bool {
        let slot = &self.tableau[pos];

        slot.last().map_or(true, |&top| card.can_top(top))
    }

    pub fn can_move_to_foundation(&self, card: Card) -> bool {
        let slot = self.foundation(card.suit);

        card.can_succeed(slot)
    }

    pub fn should_move_to_foundation(&self, card: Card) -> bool {
        if !self.can_move_to_foundation(card) {
            return false;
        }

        let club_v =    self.foundation(Suit::Club)   .map_or(0, |c| c.value.0);
        let space_v =   self.foundation(Suit::Spade)  .map_or(0, |c| c.value.0);
        let diamond_v = self.foundation(Suit::Diamond).map_or(0, |c| c.value.0);
        let heart_v =   self.foundation(Suit::Heart)  .map_or(0, |c| c.value.0);

        let min_black = min(club_v, space_v);
        let min_red = min(diamond_v, heart_v);

        if card.suit.color() == Color::Black {
            card.value.0 <= min(min_black + 3, min_red + 2)
        } else {
            card.value.0 <= min(min_red + 3, min_black + 2)
        }
    }

    /// Returns whether any reserve slots are vacant.
    pub fn reserve_free(&self) -> bool {
        self.reserve.iter().any(|r| r.is_none())
    }

    pub fn game_over(&self) -> bool {
        self.foundation.iter().all(
            |f| f.map_or(false, |c| c.value.0 == KING))
    }

    pub fn add_to_foundation(&mut self, card: Card) {
        self.assert_free(card);
        assert!(self.can_move_to_foundation(card));

        let slot = self.foundation_mut(card.suit);
        *slot = Some(card);
    }

    pub fn add_to_tableau(&mut self, card: Card, pos: usize) {
        self.assert_free(card);
        assert!(self.can_move_to_tableau(card, pos));

        self.tableau[pos].push(card);
    }

    pub fn move_tableau_group(&mut self, a: usize, b: usize, n: usize) {
        assert!(n != 0);
        assert!(a != b);
        assert!(n <= self.move_capacity(a, b));

        let (a, b) = two_mut_refs(&mut self.tableau, a, b);

        let start = a.len() - n;
        b.extend(a.drain(start..));
    }

    pub fn add_to_reserve(&mut self, card: Card) {
        self.assert_free(card);

        match self.reserve.iter_mut().find(|r| r.is_none()) {
            Some(r) => *r = Some(card),
            None => panic!("reserve is full")
        }
    }

    /// Automatically moves to foundation up to `n` cards.
    /// Returns whether any cards were moved.
    pub fn sweep_step(&mut self, n: u32) -> bool {
        let mut left = n;

        for (n, r) in self.reserve.clone().iter().cloned().enumerate() {
            if let Some(c) = r {
                if self.should_move_to_foundation(c) {
                    self.remove_reserve(n);
                    self.add_to_foundation(c);

                    left -= 1;
                    if left == 0 {
                        break;
                    }
                }
            }
        }

        if left != 0 {
            let sweep = self.tableau.iter().cloned().enumerate()
                .filter_map(|(i, t)| t.last().map(|&c| (i, c)))
                .filter(|&(_, c)| self.should_move_to_foundation(c))
                .map(|(i, _)| i).collect::<Vec<_>>();

            for i in sweep.into_iter().take(left as usize) {
                let c = self.pop_tableau(i);
                self.add_to_foundation(c);

                left -= 1;
            }
        }

        left != n
    }

    pub fn remove_reserve(&mut self, pos: usize) -> Card {
        self.reserve[pos].take().expect("reserve is empty")
    }

    pub fn reserve_slots(&self) -> &[Option<Card>] { &self.reserve }

    pub fn reserve(&self, pos: usize) -> Option<Card> {
        self.reserve[pos]
    }

    pub fn tableau_slots(&self) -> &[Vec<Card>] { &self.tableau }

    pub fn tableau(&self, pos: usize) -> &[Card] {
        &self.tableau[pos]
    }

    pub fn tableau_mut(&mut self, pos: usize) -> &mut Vec<Card> {
        &mut self.tableau[pos]
    }

    pub fn pop_tableau(&mut self, pos: usize) -> Card {
        self.tableau[pos].pop().expect("tableau is empty")
    }

    fn assert_free(&self, card: Card) {
        assert!(self.reserve.iter().all(|&r| r != Some(card)),
            "card is not free; found in reserve");
        assert!(!self.foundation.iter().any(
            |r| r.map_or(false, |r| card.is_lower(r))),
            "card is not free; found in foundation");
        assert!(self.tableau.iter().all(|t| !t.contains(&card)),
            "card is not free; found in tableau");
    }

    pub fn foundation_slots(&self) -> &[Option<Card>] { &self.foundation }

    pub fn foundation(&self, suit: Suit) -> Option<Card> {
        self.foundation[suit.as_index()]
    }

    fn foundation_mut(&mut self, suit: Suit) -> &mut Option<Card> {
        &mut self.foundation[suit.as_index()]
    }

    pub fn group_size(&self, pos: usize) -> usize {
        let slot = &self.tableau[pos];

        if slot.is_empty() {
            return 0;
        }
        if slot.len() == 1 {
            return 1;
        }

        let mut n = 1;

        let pairs = slot.iter().zip(slot[1..].iter());

        for (&a, &b) in pairs.rev() {
            if b.can_top(a) {
                n += 1;
            } else {
                break;
            }
        }

        n
    }

    pub fn move_capacity(&self, a: usize, b: usize) -> usize {
        assert!(a != b);

        let slot_a = &self.tableau[a];
        let slot_b = &self.tableau[b];

        assert!(!slot_a.is_empty());

        let mut n_empty = self.tableau.iter()
            .filter(|t| t.is_empty()).count();

        if slot_b.is_empty() {
            n_empty -= 1;
        }

        let n_reserve = self.reserve.iter()
            .filter(|r| r.is_none()).count();

        min(self.group_size(a),
            (n_reserve + 1) * 2usize.pow(n_empty as u32))
    }
}

fn two_mut_refs<T>(slice: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert!(a != b);

    if a < b {
        let (aa, bb) = slice.split_at_mut(b);
        (&mut aa[a], &mut bb[0])
    } else {
        let (bb, aa) = slice.split_at_mut(a);
        (&mut aa[0], &mut bb[b])
    }
}
