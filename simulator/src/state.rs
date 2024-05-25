use crate::{Action, ComboAction, Condition, Effects, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    InProgress(InProgress),
    Completed { missing_quality: u32 },
    Failed { missing_progress: u32 },
    Invalid,
}

impl State {
    pub fn new(settings: &Settings) -> State {
        State::InProgress(InProgress::new(settings))
    }

    pub fn use_actions(
        self,
        actions: &[Action],
        condition: Condition,
        settings: &Settings,
    ) -> State {
        let mut current_state = self;
        for action in actions {
            match current_state {
                State::InProgress(in_progress) => {
                    current_state = in_progress.use_action(*action, condition, settings);
                }
                _ => return State::Invalid,
            }
        }
        current_state
    }

    pub fn as_in_progress(self) -> Option<InProgress> {
        match self {
            State::InProgress(in_progress) => Some(in_progress),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InProgress {
    pub cp: i16,
    pub durability: i16,
    pub missing_progress: u32,
    pub missing_quality: u32,
    pub effects: Effects,
    pub combo: Option<ComboAction>,
}

impl InProgress {
    pub fn new(settings: &Settings) -> InProgress {
        InProgress {
            cp: settings.max_cp,
            durability: settings.max_durability,
            missing_progress: settings.max_progress,
            missing_quality: settings.max_quality,
            effects: Default::default(),
            combo: Some(ComboAction::SynthesisBegin),
        }
    }

    fn can_use_action(&self, action: Action, condition: Condition) -> bool {
        if action.cp_cost(&self.effects, condition) > self.cp {
            return false;
        }
        if action.required_combo().is_some() && self.combo != action.required_combo() {
            return false;
        }
        match action {
            Action::ByregotsBlessing => self.effects.inner_quiet != 0,
            Action::PrudentSynthesis | Action::PrudentTouch => self.effects.waste_not == 0,
            Action::IntensiveSynthesis | Action::PreciseTouch | Action::TricksOfTheTrade => {
                condition == Condition::Good || condition == Condition::Excellent
            }
            Action::Groundwork => {
                self.durability >= action.durability_cost(&self.effects, condition)
            }
            Action::TrainedFinesse => self.effects.inner_quiet == 10,
            _ => true,
        }
    }

    pub fn use_action(&self, action: Action, condition: Condition, settings: &Settings) -> State {
        if !self.can_use_action(action, condition) {
            return State::Invalid;
        }

        let cp_cost = action.cp_cost(&self.effects, condition);
        let durability_cost = action.durability_cost(&self.effects, condition);
        let progress_increase = action.progress_increase(settings, &self.effects, condition);
        let quality_increase = action.quality_increase(settings, &self.effects, condition);

        let mut new_state = *self;
        new_state.combo = action.to_combo();
        new_state.cp -= cp_cost;
        new_state.durability -= durability_cost;

        // reset muscle memory if progress increased
        if progress_increase != 0 {
            new_state.missing_progress =
                new_state.missing_progress.saturating_sub(progress_increase);
            new_state.effects.muscle_memory = 0;
        }

        // reset great strides and increase inner quiet if quality increased
        if quality_increase != 0 {
            new_state.missing_quality = new_state.missing_quality.saturating_sub(quality_increase);
            new_state.effects.great_strides = 0;
            if settings.job_level >= 11 {
                new_state.effects.inner_quiet += match action {
                    Action::Reflect => 2,
                    Action::PreciseTouch => 2,
                    Action::PreparatoryTouch => 2,
                    _ => 1,
                };
                new_state.effects.inner_quiet = std::cmp::min(10, new_state.effects.inner_quiet);
            }
        }

        if new_state.missing_progress == 0 {
            return State::Completed {
                missing_quality: new_state.missing_quality,
            };
        }
        if new_state.durability <= 0 {
            return State::Failed {
                missing_progress: new_state.missing_progress,
            };
        }

        // remove manipulation before it is triggered
        if action == Action::Manipulation {
            new_state.effects.manipulation = 0;
        }

        if new_state.effects.manipulation > 0 {
            new_state.durability = std::cmp::min(new_state.durability + 5, settings.max_durability);
        }
        new_state.effects.tick_down();

        // trigger special action effects
        let duration_bonus = if condition == Condition::Pliant { 2 } else { 0 };
        match action {
            Action::MuscleMemory => new_state.effects.muscle_memory = 5 + duration_bonus,
            Action::GreatStrides => new_state.effects.great_strides = 3 + duration_bonus,
            Action::Veneration => new_state.effects.veneration = 4 + duration_bonus,
            Action::Innovation => new_state.effects.innovation = 4 + duration_bonus,
            Action::WasteNot => new_state.effects.waste_not = 4 + duration_bonus,
            Action::WasteNot2 => new_state.effects.waste_not = 8 + duration_bonus,
            Action::Manipulation => new_state.effects.manipulation = 8 + duration_bonus,
            Action::MasterMend => {
                new_state.durability =
                    std::cmp::min(settings.max_durability, new_state.durability + 30)
            }
            Action::ByregotsBlessing => new_state.effects.inner_quiet = 0,
            Action::TricksOfTheTrade => {
                new_state.cp = std::cmp::min(settings.max_cp, new_state.cp + 20)
            }
            _ => (),
        }

        State::InProgress(new_state)
    }
}