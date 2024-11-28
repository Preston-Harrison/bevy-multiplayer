use bevy::{color::palettes::css::WHITE, prelude::*};

const MESSAGES: usize = 5;
const FONT_SIZE: f32 = 20.0;

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ConsoleMessage>()
            .add_systems(Startup, setup)
            .add_systems(Update, render_console);
    }
}

#[derive(Event, Clone)]
pub struct ConsoleMessage {
    content: String,
    color: Color,
}

impl ConsoleMessage {
    pub fn new(content: String) -> Self {
        Self {
            content: content + "\n",
            color: WHITE.into(),
        }
    }
}

#[derive(Component)]
struct ConsoleTag;

fn setup(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                bottom: Val::Px(5.0),
                left: Val::Px(5.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn((
                TextBundle::from_sections(
                    std::iter::repeat_with(|| TextSection::default()).take(MESSAGES),
                ),
                ConsoleTag,
            ));
        });
}

fn render_console(
    mut reader: EventReader<ConsoleMessage>,
    mut console: Query<&mut Text, With<ConsoleTag>>,
) {
    let Ok(mut console) = console.get_single_mut() else {
        return;
    };
    for msg in reader.read() {
        let mut new_sections = console.sections.clone();
        for i in 1..MESSAGES {
            new_sections[i - 1] = new_sections[i].clone();
        }
        new_sections[MESSAGES - 1] = TextSection::new(
            msg.content.clone(),
            TextStyle {
                color: msg.color,
                font_size: FONT_SIZE,
                ..Default::default()
            },
        );
        console.sections = new_sections;
    }
}
