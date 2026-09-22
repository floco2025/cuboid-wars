use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use super::{
    script::{Action, Script},
    session::Session,
};

pub(super) struct Executor {
    pub script: Script,
    pub session: Session,
    pub initial: Value,
    pub steps: Vec<Value>,
    remaining: u32,
    advanced: u32,
}

impl Executor {
    pub fn new(script: Script) -> Result<Self> {
        let session = Session::new(&script)?;
        let initial = session.state();
        Ok(Self {
            script,
            session,
            initial,
            steps: Vec::new(),
            remaining: 0,
            advanced: 0,
        })
    }

    pub fn running(&self) -> bool {
        self.remaining > 0
    }
    pub fn finished(&self) -> bool {
        self.steps.len() == self.script.actions.len()
    }

    // Both frontends execute exactly these action boundaries and ticks. Pacing
    // never enters the simulation, and an inspection pause calls neither method.
    pub fn start_next(&mut self) -> Result<()> {
        ensure!(!self.running(), "an action is already running");
        if self.finished() {
            return Ok(());
        }
        self.session.events.clear();
        self.advanced = 0;
        let result = match self.script.actions[self.steps.len()].clone() {
            Action::Move {
                direction,
                ticks,
                run,
                jump,
            } => {
                self.session.begin_move(direction, run, jump);
                self.remaining = ticks;
                return Ok(());
            }
            Action::Advance { ticks } => {
                self.remaining = ticks;
                return Ok(());
            }
            Action::Aim { target } => self.session.aim(target),
            Action::Portal { end } => self.session.portal(end),
            Action::Fire => self.session.fire(),
            Action::Check { min, max, grounded } => Ok(self.session.check(min, max, grounded)),
            Action::Inspect => Ok(json!({"status":"inspected"})),
            Action::Reset => {
                let visual = self.session.visual_messages.is_some();
                self.session = Session::new(&self.script)?;
                self.session.visual_messages = visual.then(Vec::new);
                Ok(json!({"status":"reset"}))
            }
        }
        .with_context(|| format!("action {}", self.steps.len()))?;
        self.complete(result);
        Ok(())
    }

    pub fn tick(&mut self) -> Result<()> {
        if !self.running() {
            return Ok(());
        }
        let moving = matches!(self.script.actions[self.steps.len()], Action::Move { .. });
        if !moving || self.session.alive() {
            self.session
                .advance()
                .with_context(|| format!("action {}", self.steps.len()))?;
            self.advanced += 1;
            self.remaining -= 1;
        }
        if self.remaining == 0 || (moving && !self.session.alive()) {
            let result = if moving {
                self.session.end_move();
                json!({"status": if self.remaining == 0 {"simulated"} else {"interrupted"}, "ticks":self.advanced})
            } else {
                json!({"status":"advanced"})
            };
            self.complete(result);
        }
        Ok(())
    }

    fn complete(&mut self, result: Value) {
        self.remaining = 0;
        self.steps.push(
            json!({"index":self.steps.len(), "action":self.script.actions[self.steps.len()],
            "result":result, "events":self.session.events, "state":self.session.state()}),
        );
    }
}
