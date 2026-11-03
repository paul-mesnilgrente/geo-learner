//! Déroulement d'une partie, indépendamment de l'interface.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Nombre d'essais avant de révéler la réponse (modes « Nommer » et « Localiser »).
pub const MAX_TRIES: u8 = 3;

/// Questions posées entre un lieu raté et sa seconde chance.
pub const RETRY_GAP: usize = 3;

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
    /// Seconde chance réussie : le lieu, raté plus tôt, est retrouvé. Le score n'en change pas.
    Recovered(usize),
    /// Rien à faire (partie terminée…).
    Ignored,
}

/// Bilan d'un lieu une fois joué : trouvé (et au bout de combien d'essais) ou raté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub country: usize,
    /// Essai de la bonne réponse (1 = du premier coup) ; `None` si raté.
    pub tries: Option<u8>,
    /// Mauvaises réponses données à la place (modes Nommer et Localiser), dans l'ordre.
    pub wrong: Vec<usize>,
    /// Modes Nommer et Localiser : temps de réponse depuis l'apparition de la question.
    /// Mode libre : moment où le lieu a été cité, depuis le début de la partie.
    pub time: Duration,
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
    /// Lieux joués, dans l'ordre où ils ont été tranchés.
    answers: Vec<Answer>,
    /// Mauvaises réponses données sur la question en cours.
    wrong: Vec<usize>,
    started: Instant,
    /// Apparition de la question en cours.
    asked: Instant,
    /// Lieux ratés qui attendent leur seconde chance (dont la question en cours, le cas échéant).
    retry: Vec<usize>,
    /// Lieux retrouvés lors de leur seconde chance.
    recovered: usize,
    /// Les lieux ratés reviennent plus tard (activé par défaut).
    second_chance: bool,
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
        let now = Instant::now();
        let mut quiz = Self {
            mode,
            marks,
            members,
            queue,
            target: None,
            wrong_tries: 0,
            mistakes: 0,
            finished: false,
            answers: Vec::new(),
            wrong: Vec::new(),
            started: now,
            asked: now,
            retry: Vec::new(),
            recovered: 0,
            second_chance: true,
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

    /// État affiché d'un lieu. Un lieu raté qui attend sa seconde chance apparaît « à trouver »,
    /// pour que la carte ne souffle pas la réponse ; il reste compté comme raté.
    pub fn mark(&self, country: usize) -> Option<Mark> {
        if self.retry.contains(&country) {
            return Some(Mark::Pending);
        }
        self.marks.get(country).copied().flatten()
    }

    /// Active ou désactive la seconde chance des lieux ratés (à régler avant de jouer).
    pub fn with_second_chance(mut self, enabled: bool) -> Self {
        self.second_chance = enabled;
        self
    }

    /// La question en cours est une seconde chance.
    pub fn retrying(&self) -> bool {
        self.target.is_some_and(|t| self.retry.contains(&t))
    }

    /// Lieux retrouvés lors de leur seconde chance.
    pub fn recovered(&self) -> usize {
        self.recovered
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

    /// Lieux joués, dans l'ordre. En modes Nommer et Localiser, les questions jamais posées
    /// (partie abandonnée avant) n'y figurent pas.
    pub fn answers(&self) -> &[Answer] {
        &self.answers
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
                    self.answers.push(Answer {
                        country,
                        tries: Some(1),
                        wrong: Vec::new(),
                        time: self.started.elapsed(),
                    });
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
                if self.retrying() {
                    return self.second_chance(target, country);
                }
                if country == target {
                    let tries = self.wrong_tries + 1;
                    self.marks[target] = Some(Mark::Found { tries });
                    self.settle(target, Some(tries));
                    self.advance();
                    return Outcome::Correct { country, tries };
                }
                self.wrong_tries += 1;
                self.mistakes += 1;
                self.wrong.push(country);
                if self.wrong_tries >= MAX_TRIES {
                    self.marks[target] = Some(Mark::Missed);
                    self.settle(target, None);
                    self.requeue(target);
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

    /// Passe le pays en cours (compté comme raté ; il reviendra en seconde chance).
    pub fn skip(&mut self) -> Option<usize> {
        let target = self.target?;
        if self.retrying() {
            self.retry.retain(|&c| c != target);
        } else {
            self.marks[target] = Some(Mark::Missed);
            self.settle(target, None);
            self.requeue(target);
        }
        self.advance();
        Some(target)
    }

    /// Réponse à une seconde chance : ni le score ni le bilan du lieu ne changent, et un nouvel
    /// échec révèle la réponse sans autre essai.
    fn second_chance(&mut self, target: usize, country: usize) -> Outcome {
        if country == target {
            self.retry.retain(|&c| c != target);
            self.recovered += 1;
            self.advance();
            return Outcome::Recovered(target);
        }
        self.wrong_tries += 1;
        if self.wrong_tries >= MAX_TRIES {
            self.retry.retain(|&c| c != target);
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

    /// Repropose un lieu raté quelques questions plus tard, s'il en reste.
    fn requeue(&mut self, target: usize) {
        if !self.second_chance {
            return;
        }
        let more = self
            .queue
            .iter()
            .any(|&c| self.marks[c] == Some(Mark::Pending) || self.retry.contains(&c));
        if more {
            let at = self.queue.len().saturating_sub(RETRY_GAP);
            self.queue.insert(at, target);
            self.retry.push(target);
        }
    }

    /// Termine la partie : tous les pays restants sont comptés comme ratés.
    ///
    /// Bilan : en mode libre, les lieux non cités sont ratés. En modes Nommer et Localiser, la
    /// question en cours compte comme ratée si une mauvaise réponse a déjà été donnée ; sinon,
    /// comme les questions jamais posées, elle ne compte pas.
    pub fn give_up(&mut self) {
        if self.finished {
            return;
        }
        match (self.mode, self.target) {
            (Mode::Free, _) => {
                let time = self.started.elapsed();
                for &c in &self.members {
                    if self.marks[c] == Some(Mark::Pending) {
                        self.answers.push(Answer {
                            country: c,
                            tries: None,
                            wrong: Vec::new(),
                            time,
                        });
                    }
                }
            }
            (_, Some(target)) if !self.wrong.is_empty() && !self.retrying() => {
                self.settle(target, None);
            }
            _ => {}
        }
        for &c in &self.members {
            if self.marks[c] == Some(Mark::Pending) {
                self.marks[c] = Some(Mark::Missed);
            }
        }
        self.queue.clear();
        self.retry.clear();
        self.target = None;
        self.finished = true;
    }

    /// Note le bilan de la question en cours.
    fn settle(&mut self, target: usize, tries: Option<u8>) {
        self.answers.push(Answer {
            country: target,
            tries,
            wrong: std::mem::take(&mut self.wrong),
            time: self.asked.elapsed(),
        });
    }

    fn advance(&mut self) {
        self.wrong_tries = 0;
        self.wrong.clear();
        self.target = None;
        while let Some(next) = self.queue.pop() {
            if self.marks[next] == Some(Mark::Pending) || self.retry.contains(&next) {
                self.target = Some(next);
                self.asked = Instant::now();
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
        // Raté, mais affiché « à trouver » en attendant sa seconde chance.
        assert_eq!(q.mark(0), Some(Mark::Pending));
        assert_eq!(q.missed(), 1);
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

    /// Résumé des bilans : (lieu, essais, mauvaises réponses).
    fn answers(q: &Quiz) -> Vec<(usize, Option<u8>, Vec<usize>)> {
        q.answers()
            .iter()
            .map(|a| (a.country, a.tries, a.wrong.clone()))
            .collect()
    }

    #[test]
    fn answers_record_tries_and_confusions() {
        let mut q = Quiz::new(Mode::Locate, 5, vec![0, 1, 2, 3], vec![0, 1, 2, 3]);
        q.submit(0);
        q.submit(4); // 1 confondu avec 4
        q.submit(1);
        q.submit(3);
        q.submit(4);
        q.submit(3); // 2 révélé
        q.skip(); // 3 passé
        assert_eq!(
            answers(&q),
            [
                (0, Some(1), vec![]),
                (1, Some(2), vec![4]),
                (2, None, vec![3, 4, 3]),
                (3, None, vec![]),
            ]
        );
    }

    #[test]
    fn give_up_records_only_asked_questions() {
        // Question en cours sans erreur, puis jamais posées : rien n'est noté.
        let mut q = Quiz::new(Mode::Identify, 3, vec![0, 1, 2], vec![0, 1, 2]);
        q.submit(0);
        q.give_up();
        assert_eq!(answers(&q), [(0, Some(1), vec![])]);
        // Question en cours avec une erreur : ratée.
        let mut q = Quiz::new(Mode::Identify, 3, vec![0, 1, 2], vec![0, 1, 2]);
        q.submit(2);
        q.give_up();
        assert_eq!(answers(&q), [(0, None, vec![2])]);
        q.give_up();
        assert_eq!(q.answers().len(), 1);
        // Mode libre : les lieux non cités sont ratés.
        let mut q = Quiz::new(Mode::Free, 3, vec![0, 1, 2], vec![]);
        q.submit(1);
        q.give_up();
        assert_eq!(
            answers(&q),
            [(1, Some(1), vec![]), (0, None, vec![]), (2, None, vec![])]
        );
    }

    #[test]
    fn missed_places_come_back_later() {
        let order: Vec<usize> = (0..6).collect();
        let mut q = Quiz::new(Mode::Identify, 6, order.clone(), order);
        assert_eq!(q.skip(), Some(0));
        // Trois autres questions d'abord, puis la seconde chance.
        for c in 1..4 {
            assert_eq!(q.target(), Some(c));
            assert!(!q.retrying());
            q.submit(c);
        }
        assert_eq!(q.target(), Some(0));
        assert!(q.retrying());
        assert_eq!(q.submit(0), Outcome::Recovered(0));
        assert_eq!(q.recovered(), 1);
        // Toujours raté : le score et le bilan ne changent pas.
        assert_eq!(q.mark(0), Some(Mark::Missed));
        assert_eq!(q.answers()[0].tries, None);
        assert_eq!(q.target(), Some(4));
        // Le dernier lieu raté n'a plus de question après lui : pas de seconde chance.
        q.submit(4);
        q.skip();
        assert!(q.is_finished());
        assert_eq!((q.found(), q.missed(), q.mistakes()), (4, 2, 0));
    }

    #[test]
    fn second_chance_can_be_disabled() {
        let order: Vec<usize> = (0..3).collect();
        let mut q = Quiz::new(Mode::Identify, 3, order.clone(), order).with_second_chance(false);
        q.skip();
        q.submit(1);
        q.submit(2);
        assert!(q.is_finished());
        assert_eq!(q.mark(0), Some(Mark::Missed));
    }

    #[test]
    fn second_chance_failure_reveals_without_requeue() {
        let order: Vec<usize> = (0..3).collect();
        let mut q = Quiz::new(Mode::Locate, 3, order.clone(), order);
        for _ in 0..MAX_TRIES {
            q.submit(2);
        }
        assert_eq!(q.target(), Some(1));
        q.submit(1);
        q.submit(2);
        assert_eq!(q.target(), Some(0));
        assert!(q.retrying());
        q.submit(1);
        q.submit(1);
        assert_eq!(
            q.submit(1),
            Outcome::Revealed {
                guess: 1,
                target: 0
            }
        );
        assert!(q.is_finished());
        assert_eq!(q.mark(0), Some(Mark::Missed));
        // Les erreurs de la seconde chance ne s'ajoutent ni aux erreurs ni au bilan.
        assert_eq!(q.mistakes(), 3);
        assert_eq!(q.answers()[0].wrong, [2, 2, 2]);
        assert_eq!(q.answers().len(), 3);
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
