#[cfg(feature = "tty-preview")]
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::compositor::DrmCompositor;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmNode, DrmSurface};
#[cfg(feature = "tty-preview")]
use smithay::backend::egl::EGLDisplay;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::gles::GlesRenderer;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::multigpu::{GpuManager, gbm::GbmGlesBackend};
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::{connector, crtc};

#[allow(dead_code)]
pub struct TtyDrmBackendState {
    #[cfg(feature = "tty-preview")]
    pub devices: Vec<DrmDeviceRecord>,
}

impl Default for TtyDrmBackendState {
    fn default() -> Self {
        Self {
            #[cfg(feature = "tty-preview")]
            devices: Vec::new(),
        }
    }
}

#[cfg(feature = "tty-preview")]
#[allow(dead_code)]
pub struct DrmDeviceRecord {
    pub node: DrmNode,
    pub device: DrmDevice,
    pub render: DrmRenderState,
    pub surfaces: Vec<DrmSurfaceRecord>,
}

#[cfg(feature = "tty-preview")]
#[allow(dead_code)]
pub struct DrmRenderState {
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub allocator: GbmAllocator<DrmDeviceFd>,
    pub framebuffer_exporter: GbmFramebufferExporter<DrmDeviceFd>,
    pub egl_display: EGLDisplay,
    pub render_node: Option<DrmNode>,
    pub gpu_manager: Option<GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>>,
}

#[cfg(feature = "tty-preview")]
#[allow(dead_code)]
pub struct DrmSurfaceRecord {
    pub connector: connector::Handle,
    pub crtc: crtc::Handle,
    pub surface: Option<DrmSurface>,
    pub compositor: Option<
        DrmCompositor<
            GbmAllocator<DrmDeviceFd>,
            GbmFramebufferExporter<DrmDeviceFd>,
            (),
            DrmDeviceFd,
        >,
    >,
}

#[cfg(feature = "tty-preview")]
pub(crate) fn wrap_drm_device(
    node: DrmNode,
    device_fd: smithay::utils::DeviceFd,
) -> Result<(DrmDeviceRecord, DrmDeviceNotifier), Box<dyn std::error::Error>> {
    let drm_fd = DrmDeviceFd::new(device_fd);
    let (device, notifier) = DrmDevice::new(drm_fd, true)?;
    let gbm = GbmDevice::new(device.device_fd().clone())?;
    let allocator =
        GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let egl_display = unsafe { EGLDisplay::new(gbm.clone()) }?;
    let render_node = smithay::backend::egl::EGLDevice::device_for_display(&egl_display)
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())
        .or(Some(node));
    let framebuffer_exporter = GbmFramebufferExporter::new(gbm.clone(), render_node.into());

    Ok((
        DrmDeviceRecord {
            node,
            device,
            render: DrmRenderState {
                gbm,
                allocator,
                framebuffer_exporter,
                egl_display,
                render_node,
                gpu_manager: None,
            },
            surfaces: Vec::new(),
        },
        notifier,
    ))
}
