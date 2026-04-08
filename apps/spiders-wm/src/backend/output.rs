#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmNode;
#[cfg(feature = "tty-preview")]
use smithay::output::Subpixel;
use smithay::output::{Mode, Output, PhysicalProperties};
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control as drm_control;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::connector;
use smithay::utils::{Logical, Point, Transform};

use crate::runtime::NoopHost;
use crate::state::SpidersWm;
use spiders_core::signal::WmSignal;

#[allow(dead_code)]
pub struct TtyOutputState {
    #[cfg(feature = "tty-preview")]
    pub output: Output,
    #[cfg(feature = "tty-preview")]
    pub output_id: String,
    #[cfg(feature = "tty-preview")]
    pub location: Point<i32, Logical>,
    pub connector_name: String,
    #[cfg(feature = "tty-preview")]
    pub drm_node: Option<DrmNode>,
    #[cfg(feature = "tty-preview")]
    pub connector: connector::Handle,
    #[cfg(feature = "tty-preview")]
    pub mode: Mode,
    #[cfg(feature = "tty-preview")]
    pub drm_mode: drm_control::Mode,
    #[cfg(feature = "tty-preview")]
    pub physical: PhysicalProperties,
}

#[cfg(feature = "tty-preview")]
impl TtyOutputState {
    pub(crate) fn same_render_state(&self, other: &Self) -> bool {
        self.output_id == other.output_id
            && self.connector_name == other.connector_name
            && self.drm_node == other.drm_node
            && self.mode == other.mode
            && self.location == other.location
            && self.physical.size == other.physical.size
            && self.physical.subpixel == other.physical.subpixel
            && self.physical.make == other.physical.make
            && self.physical.model == other.physical.model
            && self.physical.serial_number == other.physical.serial_number
    }
}

#[cfg(feature = "tty-preview")]
pub(crate) fn assign_tty_output_locations(
    outputs: &mut [TtyOutputState],
    previous_outputs: &[TtyOutputState],
) {
    let mut next_x = previous_outputs
        .iter()
        .map(|output| output.location.x + output.mode.size.w)
        .max()
        .unwrap_or(0);

    for output in outputs {
        if let Some(previous) =
            previous_outputs.iter().find(|candidate| candidate.output_id == output.output_id)
        {
            output.location = previous.location;
            continue;
        }

        output.location = (next_x, 0).into();
        next_x += output.mode.size.w;
    }
}

pub(crate) struct OutputRegistration {
    pub output_id: String,
    pub output_name: String,
    pub mode: Mode,
    pub transform: Transform,
    pub location: Point<i32, Logical>,
    pub physical: PhysicalProperties,
}

pub(crate) fn register_output(state: &mut SpidersWm, registration: OutputRegistration) -> Output {
    let output = Output::new(registration.output_name.clone(), registration.physical);
    let _global = output.create_global::<SpidersWm>(&state.display_handle);
    output.change_current_state(
        Some(registration.mode),
        Some(registration.transform),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(registration.mode);
    state.space.map_output(&output, registration.location);
    state.space.refresh();
    state.refresh_fractional_scale_for_mapped_surfaces();
    sync_output_to_runtime(
        state,
        registration.output_id,
        registration.output_name,
        registration.mode,
    );
    output
}

#[cfg(feature = "tty-preview")]
pub(crate) fn register_existing_output(
    state: &mut SpidersWm,
    output_id: String,
    output_name: String,
    output: &Output,
    mode: Mode,
    transform: Transform,
    location: Point<i32, Logical>,
) {
    let _global = output.create_global::<SpidersWm>(&state.display_handle);
    output.change_current_state(Some(mode), Some(transform), None, Some(location));
    output.set_preferred(mode);
    state.space.map_output(output, location);
    state.space.refresh();
    state.refresh_fractional_scale_for_mapped_surfaces();
    sync_output_to_runtime(state, output_id, output_name, mode);
}

#[cfg(feature = "tty-preview")]
pub(crate) fn drm_physical_properties(make: String, model: String) -> PhysicalProperties {
    PhysicalProperties {
        size: (0, 0).into(),
        subpixel: Subpixel::Unknown,
        make,
        model,
        serial_number: "Unknown".into(),
    }
}

pub(crate) fn sync_output_to_runtime(
    state: &mut SpidersWm,
    output_id: String,
    output_name: String,
    mode: Mode,
) {
    let config = state.config.clone();
    let events = {
        let mut runtime = state.runtime();
        let mut events = runtime.handle_signal(
            &mut NoopHost,
            WmSignal::OutputSynced {
                output_id: output_id.into(),
                name: output_name,
                logical_width: mode.size.w as u32,
                logical_height: mode.size.h as u32,
            },
        );
        runtime.sync_layout_selection_defaults(&config);
        events.extend(runtime.take_events());
        events
    };
    state.broadcast_runtime_events(events);
}

#[cfg(feature = "tty-preview")]
pub(crate) fn remove_output_from_runtime(state: &mut SpidersWm, output_id: String) {
    let events = {
        let mut runtime = state.runtime();
        runtime
            .handle_signal(&mut NoopHost, WmSignal::OutputRemoved { output_id: output_id.into() })
    };
    state.broadcast_runtime_events(events);
}
