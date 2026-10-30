//! Déroulement d'une partie, indépendamment de l'interface.

use serde::{Deserialize, Serialize};

/// Nombre d'essais avant de révéler la réponse (modes « Nommer » et « Localiser »).
pub const MAX_TRIES: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    /// Citer tous les pays de la zone, dans n'importe quel ordre.
    Free,
    /// Un pays est mis en évidence sur la carte : il faut le nommer.
    Identify,
    /// Un nom de pays est donné : il faut cliquer dessus.
    Locate,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Free, Mode::Identify, Mode::Locate];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Pending,
    /// Trouvé au `tries`-ième essai.
    Found {
        tries: u8,
    },
    Missed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Mode libre : nouveau pays trouvé.
    Found(usize),
    AlreadyFound(usize),
    /// Le pays existe mais ne fait pas partie de la zone jouée.
    OutOfScope(usize),
    Correct {
        country: usize,
        tries: u8,
    },
    Wrong {
        guess: usize,
        target: usize,
    },
    /// Trop d'erreurs : la réponse est révélée et on passe au pays suivant.
    Revealed {
        guess: usize,
        target: usize,
    },
    /// Rien à faire (partie terminée…).
    Ignored,
}

#[derive(Debug, Clone)]
pub struct Quiz {
    mode: Mode,
    marks: Vec<Option<Mark>>,
    members: Vec<usize>,
    /// Pays restant à proposer ; le prochain est à la fin.
    queue: Vec<usize>,
    target: Option<usize>,
    wrong_tries: u8,
    mistakes: u32,
    finished: bool,
}

impl Quiz {
    /// `order` donne l'ordre de passage des pays (modes Nommer et Localiser) ;
    /// il doit contenir les mêmes pays que `members`.
    pub fn new(mode: Mode, country_count: usize, members: Vec<usize>, order: Vec<usize>) -> Self {
        let mut marks = vec![None; country_count];
        for &m in &members {
            marks[m] = Some(Mark::Pending);
        }
        let mut queue = order;
        queue.reverse();
        let mut quiz = Self {
            mode,
            marks,
            members,
            queue,
            target: None,
            wrong_tries: 0,
            mistakes: 0,
            finished: false,
        };
        if mode != Mode::Free {
            quiz.advance();
        }
        quiz
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn members(&self) -> &[usize] {
        &self.members
    }

    pub fn mark(&self, country: usize) -> Option<Mark> {
        self.marks.get(country).copied().flatten()
    }

    pub fn target(&self) -> Option<usize> {
        self.target
    }

    /// Erreurs commises sur le pays en cours.
    pub fn wrong_tries(&self) -> u8 {
        self.wrong_tries
    }

    pub fn mistakes(&self) -> u32 {
        self.mistakes
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn total(&self) -> usize {
        self.members.len()
    }

    pub fn found(&self) -> usize {
        self.count(|m| matches!(m, Mark::Found { .. }))
    }

    /// Pays trouvés du premier coup.
    pub fn perfect(&self) -> usize {
        self.count(|m| m == Mark::Found { tries: 1 })
    }

    pub fn missed(&self) -> usize {
        self.count(|m| m == Mark::Missed)
    }

    /// Pays déjà joués (trouvés ou ratés).
    pub fn done(&self) -> usize {
        self.count(|m| m != Mark::Pending)
    }

    /// Score entre 0 et 1 : 1 point au premier essai, ½ au deuxième, ¼ au troisième.
    pub fn score(&self) -> f32 {
        if self.members.is_empty() {
            return 0.0;
        }
        let points: f32 = self
            .members
            .iter()
            .map(|&c| match self.marks[c] {
                Some(Mark::Found { tries }) => 1.0 / f32::from(1u8 << (tries - 1).min(7)),
                _ => 0.0,
            })
            .sum();
        points / self.members.len() as f32
    }

    /// Une réponse (tapée, dictée ou cliquée) désignant `country`.
    pub fn submit(&mut self, country: usize) -> Outcome {
        if self.finished {
            return Outcome::Ignored;
        }
        match self.mode {
            Mode::Free => match self.mark(country) {
                None => Outcome::OutOfScope(country),
                Some(Mark::Pending) => {
                    self.marks[country] = Some(Mark::Found { tries: 1 });
                    if self.done() == self.total() {
                        self.finished = true;
                    }
                    Outcome::Found(country)
                }
                Some(_) => Outcome::AlreadyFound(country),
            },
            Mode::Identify | Mode::Locate => {
                let Some(target) = self.target else {
                    return Outcome::Ignored;
                };
                if country == target {
                    let tries = self.wrong_tries + 1;
                    self.marks[target] = Some(Mark::Found { tries });
                    self.advance();
                    return Outcome::Correct { country, tries };
                }
                self.wrong_tries += 1;
                self.mistakes += 1;
                if self.wrong_tries >= MAX_TRIES {
                    self.marks[target] = Some(Mark::Missed);
                    self.advance();
                    Outcome::Revealed {
                        guess: country,
                        target,
                    }
                } else {
                    Outcome::Wrong {
                        guess: country,
                        target,
                    }
                }
            }
        }
    }

    /// Passe le pays en cours (compté comme raté).
    pub fn skip(&mut self) -> Option<usize> {
        let target = self.target?;
        self.marks[target] = Some(Mark::Missed);
        self.advance();
        Some(target)
    }

    /// Termine la partie : tous les pays restants sont comptés comme ratés.
    pub fn give_up(&mut self) {
        for &c in &self.members {
            if self.marks[c] == Some(Mark::Pending) {
                self.marks[c] = Some(Mark::Missed);
            }
        }
        self.queue.clear();
        self.target = None;
        self.finished = true;
    }

    fn advance(&mut self) {
        self.wrong_tries = 0;
        self.target = None;
        while let Some(next) = self.queue.pop() {
            if self.marks[next] == Some(Mark::Pending) {
                self.target = Some(next);
                return;
            }
        }
        self.finished = true;
    }

    fn count(&self, pred: impl Fn(Mark) -> bool) -> usize {
        self.members
            .iter()
            .filter(|&&c| self.marks[c].is_some_and(&pred))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_mode_finds_each_country_once() {
        let mut q = Quiz::new(Mode::Free, 5, vec![1, 3], vec![]);
        assert_eq!(q.target(), None);
        assert_eq!(q.submit(0), Outcome::OutOfScope(0));
        assert_eq!(q.submit(1), Outcome::Found(1));
        assert_eq!(q.submit(1), Outcome::AlreadyFound(1));
        assert!(!q.is_finished());
        assert_eq!(q.submit(3), Outcome::Found(3));
        assert!(q.is_finished());
        assert_eq!(q.score(), 1.0);
    }

    #[test]
    fn targets_follow_given_order() {
        let mut q = Quiz::new(Mode::Identify, 4, vec![0, 1, 2], vec![2, 0, 1]);
        assert_eq!(q.target(), Some(2));
        assert_eq!(
            q.submit(2),
            Outcome::Correct {
                country: 2,
                tries: 1
            }
        );
        assert_eq!(q.target(), Some(0));
    }

    #[test]
    fn wrong_answers_then_reveal() {
        let mut q = Quiz::new(Mode::Locate, 3, vec![0, 1], vec![0, 1]);
        assert_eq!(
            q.submit(1),
            Outcome::Wrong {
                guess: 1,
                target: 0
            }
        );
        assert_eq!(
            q.submit(2),
            Outcome::Wrong {
                guess: 2,
                target: 0
            }
        );
        assert_eq!(
            q.submit(1),
            Outcome::Revealed {
                guess: 1,
                target: 0
            }
        );
        assert_eq!(q.mark(0), Some(Mark::Missed));
        assert_eq!(q.target(), Some(1));
        assert_eq!(q.wrong_tries(), 0);
        assert_eq!(q.mistakes(), 3);
    }

    #[test]
    fn score_rewards_first_tries() {
        let mut q = Quiz::new(Mode::Identify, 2, vec![0, 1], vec![0, 1]);
        q.submit(0);
        q.submit(0); // erreur sur 1
        q.submit(1);
        assert!(q.is_finished());
        assert_eq!(q.perfect(), 1);
        assert_eq!(q.score(), 0.75);
    }

    #[test]
    fn skip_and_give_up() {
        let mut q = Quiz::new(Mode::Identify, 3, vec![0, 1, 2], vec![0, 1, 2]);
        assert_eq!(q.skip(), Some(0));
        assert_eq!(q.target(), Some(1));
        q.give_up();
        assert!(q.is_finished());
        assert_eq!(q.missed(), 3);
        assert_eq!(q.submit(1), Outcome::Ignored);
    }
}
