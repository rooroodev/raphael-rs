use radix_heap::RadixHeapMap;
use rustc_hash::FxHashMap;

use crate::actions::{DURABILITY_ACTIONS, MIXED_ACTIONS, PROGRESS_ACTIONS, QUALITY_ACTIONS};
use crate::{FinishSolver, UpperBoundSolver};
use simulator::{state::InProgress, Action, ActionMask, Condition, Settings, State};

use std::time::Instant;
use std::vec::Vec;

const SEARCH_ACTIONS: ActionMask = PROGRESS_ACTIONS
    .union(QUALITY_ACTIONS)
    .union(MIXED_ACTIONS)
    .union(DURABILITY_ACTIONS);

pub struct MacroSolver {
    settings: Settings,
    finish_solver: FinishSolver,
    bound_solver: UpperBoundSolver,
}

impl MacroSolver {
    pub fn new(settings: Settings) -> MacroSolver {
        dbg!(std::mem::size_of::<SearchNode>());
        dbg!(std::mem::align_of::<SearchNode>());
        MacroSolver {
            settings,
            finish_solver: FinishSolver::new(settings),
            bound_solver: UpperBoundSolver::new(settings),
        }
    }

    /// Returns a list of Actions that maximizes Quality of the completed state.
    /// Returns `None` if the state cannot be completed (i.e. cannot max out Progress).
    /// The solver makes an effort to produce a short solution, but it is not (yet) guaranteed to be the shortest solution.
    pub fn solve(&mut self, state: State) -> Option<Vec<Action>> {
        match state {
            State::InProgress(state) => {
                let timer = Instant::now();
                if !self.finish_solver.can_finish(&state) {
                    return None;
                }
                let seconds = timer.elapsed().as_secs_f32();
                dbg!(seconds);
                match self.do_solve(state) {
                    Some(actions) => Some(actions),
                    None => Some(self.finish_solver.get_finish_sequence(state).unwrap()),
                }
            }
            _ => None,
        }
    }

    fn do_solve(&mut self, state: InProgress) -> Option<Vec<Action>> {
        let timer = Instant::now();

        let mut finish_solver_rejected_node: usize = 0;
        let mut upper_bound_solver_rejected_nodes: usize = 0;

        // key: State::InProgress (with missing_quality set to 0)
        // value: min missing_quality seen for the key
        let mut visited_states = FxHashMap::default();

        // priority queue based on quality upper bound
        let mut search_queue = RadixHeapMap::new();

        // backtracking data
        let mut traces: Vec<Option<SearchTrace>> = Vec::new();

        let mut best_quality = 0;
        let mut best_state = None;
        let mut best_trace = 0;

        visited_states.insert(hash_key(state), state.missing_quality);
        search_queue.push(
            self.bound_solver.quality_upper_bound(state),
            SearchNode {
                state,
                backtrack_index: 0,
            },
        );
        traces.push(None);

        while let Some((quality_bound, node)) = search_queue.pop() {
            if best_quality == self.settings.max_quality || quality_bound <= best_quality {
                continue;
            }
            for action in SEARCH_ACTIONS
                .intersection(self.settings.allowed_actions)
                .actions_iter()
            {
                let state = node
                    .state
                    .use_action(action, Condition::Normal, &self.settings);
                if let State::InProgress(state) = state {
                    // skip this state if we already visited the same state but with equal or more Quality
                    if let Some(missing_quality) = visited_states.get(&hash_key(state)) {
                        if *missing_quality <= state.missing_quality {
                            continue;
                        }
                    }

                    // skip this state if it is impossible to max out Progress
                    if !self.finish_solver.can_finish(&state) {
                        finish_solver_rejected_node += 1;
                        continue;
                    }

                    // skip this state if its Quality upper bound is not greater than the current best Quality
                    let quality_bound = self.bound_solver.quality_upper_bound(state);
                    if quality_bound <= best_quality {
                        upper_bound_solver_rejected_nodes += 1;
                        continue;
                    }

                    visited_states.insert(hash_key(state), state.missing_quality);
                    search_queue.push(
                        quality_bound,
                        SearchNode {
                            state,
                            backtrack_index: traces.len(),
                        },
                    );
                    traces.push(Some(SearchTrace {
                        parent: node.backtrack_index,
                        action,
                    }));

                    let quality = self.settings.max_quality - state.missing_quality;
                    if quality > best_quality {
                        best_quality = quality;
                        best_state = Some(state);
                        best_trace = traces.len() - 1;
                    }
                }
            }
        }

        let best_actions = match best_state {
            Some(best_state) => {
                let trace_actions = get_actions(&traces, best_trace);
                let finish_actions = self.finish_solver.get_finish_sequence(best_state).unwrap();
                Some(trace_actions.chain(finish_actions).collect())
            }
            None => None,
        };

        let seconds = timer.elapsed().as_secs_f32();
        dbg!(seconds);

        dbg!(
            traces.len(),
            finish_solver_rejected_node,
            upper_bound_solver_rejected_nodes
        );

        dbg!(best_quality, &best_actions);
        best_actions
    }
}

#[derive(Debug, Clone)]
struct SearchNode {
    pub state: InProgress,
    pub backtrack_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct SearchTrace {
    pub parent: usize,
    pub action: Action,
}

fn get_actions(traces: &[Option<SearchTrace>], mut index: usize) -> impl Iterator<Item = Action> {
    let mut actions = Vec::new();
    while let Some(trace) = traces[index] {
        actions.push(trace.action);
        index = trace.parent;
    }
    actions.into_iter().rev()
}

fn hash_key(state: InProgress) -> InProgress {
    InProgress {
        missing_quality: 0,
        ..state
    }
}