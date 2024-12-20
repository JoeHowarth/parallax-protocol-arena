use crate::prelude::*;

// #[derive(Component, Reflect, Debug)]
// pub struct KeyboardController;

pub struct KeyboardControllerPlugin;

#[derive(Resource, Deref, DerefMut, Reflect)]
struct SelectedCraft(pub Option<Entity>);

impl Plugin for KeyboardControllerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SelectedCraft>()
            .insert_resource(SelectedCraft(None))
            .add_systems(Update, update);
    }
}

#[derive(Default)]
enum SelectionState {
    #[default]
    Default,
    Target(
        Box<
            dyn FnOnce(Entity, &mut Commands, &mut SelectionState)
                + 'static
                + Send,
        >,
    ),
}

fn update(
    //
    mut commands: Commands,
    mut clicks: EventReader<Pointer<Click>>,
    crafts: Query<&CraftKind>,
    mut selected: ResMut<SelectedCraft>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: Local<SelectionState>,
) {
    for click in clicks.read() {
        info!("Click {click}");
        if !matches!(click.button, PointerButton::Primary) {
            info!("Not a primary btn");
            continue;
        }

        let state_val = std::mem::take(&mut *state);
        match state_val {
            SelectionState::Default => {
                let Ok(kind) = crafts.get(click.target) else {
                    info!("Did not click on a craft");
                    continue;
                };
                info!("Selected {:?} {kind}", click.target);

                // cleanup anything before deselect
                if let Some(prev) = **selected {
                    commands.entity(prev).remove::<KeyboardFlightController>();
                }
                // change selection
                **selected = Some(click.target);
            }
            SelectionState::Target(on_select_target) => {
                on_select_target(click.target, &mut commands, &mut state);
                *state = SelectionState::Default;
            }
        }
    }

    let Some(e) = selected.0 else {
        return;
    };
    // check if selected entity exists
    if commands.get_entity(e).is_none() {
        selected.0 = None;
        return;
    }

    let kind = crafts.get(e).unwrap();

    if matches!(*state, SelectionState::Target(..)) {
        if keys.just_pressed(KeyCode::Escape) {
            info!("Clearing target selection state");
            *state = SelectionState::Default;
        }
    }

    if keys.just_pressed(KeyCode::KeyM) {
        *state =
            SelectionState::Target(Box::new(move |target, commands, state| {
                info!("Sending 'FireMissile' event from keyboard controller");
                commands.add(send_event(FireMissile { from: e, target }));
            }));
    }
}
