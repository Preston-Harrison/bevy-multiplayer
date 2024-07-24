use bevy::prelude::*;

#[derive(Clone)]
pub struct Animation {
    id: &'static str,
    timer: Timer,
    frame: usize,
    start_ix: usize,
    end_ix: usize,
    repeat: bool,
}

impl Animation {
    pub fn new(id: &'static str, fps: f32, first_ix: usize, last_ix: usize, repeat: bool) -> Self {
        Self {
            id,
            timer: Timer::from_seconds(1.0 / fps, TimerMode::Repeating),
            frame: first_ix,
            start_ix: first_ix,
            end_ix: last_ix + 1,
            repeat,
        }
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    fn reset(&mut self) {
        self.frame = self.start_ix;
        self.timer.reset();
        self.timer.unpause();
    }

    fn get_next_frame(&self) -> usize {
        self.start_ix + (((self.frame - self.start_ix) + 1) % (self.end_ix - self.start_ix))
    }
}

#[derive(Component)]
pub struct Animator {
    current: Animation,
}

impl Animator {
    pub fn new(current: Animation) -> Self {
        Self { current }
    }

    pub fn play(&mut self, anim: Animation) {
        self.current = anim;
    }

    pub fn current(&self) -> &Animation {
        &self.current
    }
}

pub fn play_animations(mut anims: Query<(&mut Animator, &mut TextureAtlas)>, time: Res<Time>) {
    for (mut animator, mut texture) in anims.iter_mut() {
        let curr = &mut animator.current;
        curr.timer.tick(time.delta());
        let next_frame = curr.get_next_frame();

        if curr.timer.just_finished() {
            if next_frame <= curr.frame && !curr.repeat {
                animator.current.timer.pause()
            } else {
                curr.frame = next_frame;
            }
        }

        texture.index = animator.current.frame;
    }
}

pub mod graph {
    use bevy::{ecs::schedule::{BoxedCondition, Condition}, utils::HashMap};

    use super::Animation;

    pub struct Graph {
        nodes: HashMap<&'static str, Animation>,
    }

    pub struct Transition<M> {
        condition: BoxedCondition<M>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_next_frame() {
        let mut anim = Animation::new("test", 10.0, 0, 3, true);
        assert_eq!(anim.get_next_frame(), 1);
        anim.frame = 1;
        assert_eq!(anim.get_next_frame(), 2);
        anim.frame = 2;
        assert_eq!(anim.get_next_frame(), 3);
        anim.frame = 3;
        assert_eq!(anim.get_next_frame(), 0);
    }
}
