#[cfg(feature = "smithay-winit")]
mod imp {
    use smithay::backend::renderer::gles::GlesRenderer;
    use smithay::backend::winit;
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::Display;
    use spiders_runtime::{
        BackendOutputSnapshot, BackendSeatSnapshot, ControllerCommand, ControllerReport,
    };
    use spiders_shared::ids::OutputId;

    use crate::smithay_adapter::SmithayAdapter;

    #[derive(Debug, thiserror::Error)]
    pub enum SmithayRuntimeError {
        #[error("winit backend init failed: {0}")]
        Winit(String),
        #[error(transparent)]
        Controller(#[from] crate::controller::ControllerCommandError),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayStartupReport {
        pub controller: ControllerReport,
        pub output_name: String,
        pub seat_name: String,
    }

    pub fn bootstrap_winit_controller<L, R>(
        controller: &mut crate::CompositorController<L, R>,
    ) -> Result<SmithayStartupReport, SmithayRuntimeError>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        let _event_loop = EventLoop::<()>::try_new()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let _display =
            Display::<()>::new().map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

        let (backend, _events) = winit::init::<GlesRenderer>()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let size = backend.window_size();

        let seat_name = String::from("smithay-winit");
        let output_name = String::from("smithay-winit-output");
        let output_id = OutputId::from(output_name.as_str());

        let command = SmithayAdapter::translate_snapshot(
            1,
            vec![BackendSeatSnapshot {
                seat_name: seat_name.clone(),
                active: true,
            }],
            vec![BackendOutputSnapshot {
                output_id: output_id.clone(),
                active: true,
            }],
            Vec::new(),
        );

        match command {
            ControllerCommand::DiscoverySnapshot(snapshot) => {
                let _ = (size.w, size.h);
                controller.apply_command(ControllerCommand::DiscoverySnapshot(snapshot))?;
            }
            other => {
                controller.apply_command(other)?;
            }
        }

        Ok(SmithayStartupReport {
            controller: controller.report(),
            output_name,
            seat_name,
        })
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{bootstrap_winit_controller, SmithayRuntimeError, SmithayStartupReport};

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, thiserror::Error)]
pub enum SmithayRuntimeError {
    #[error("smithay-winit feature is disabled")]
    Disabled,
}

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayStartupReport;
