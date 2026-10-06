//! Real VST3 view callbacks with a mock fixed-scale editor (no OpenGL needed).
use crate::*;
use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait};
use vst3::Steinberg::{
    kInvalidArgument, kResultFalse, kResultOk, IPlugFrame, IPlugFrameTrait, IPlugView,
    IPlugViewContentScaleSupport, IPlugViewContentScaleSupportTrait, IPlugViewTrait,
    IPluginBaseTrait, ViewRect,
};
use vst3::{Class, ComRef, ComWrapper};

static OPEN: AtomicBool = AtomicBool::new(false);
static SPAWNS: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static SCALE: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());

struct FixedScaleEditor;
struct MockHandle;
impl Drop for MockHandle {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
impl Editor for FixedScaleEditor {
    fn spawn(&self, parent: ParentWindowHandle, _: Arc<dyn GuiContext>) -> Box<dyn Any + Send> {
        assert!(matches!(parent, ParentWindowHandle::Win32Hwnd(p) if p as usize == 1));
        assert!(!OPEN.swap(true, Ordering::SeqCst));
        SPAWNS.fetch_add(1, Ordering::SeqCst);
        Box::new(MockHandle)
    }
    fn size(&self) -> (u32, u32) {
        let scale = f32::from_bits(SCALE.load(Ordering::SeqCst)) as f64;
        (
            560.min((900.0 / scale) as u32),
            420.min((600.0 / scale) as u32),
        )
    }
    fn set_scale_factor(&self, factor: f32) -> bool {
        if !factor.is_finite() || !(0.5..=3.0).contains(&factor) {
            return false;
        }
        if OPEN.load(Ordering::SeqCst) {
            return SCALE.load(Ordering::SeqCst) == factor.to_bits();
        }
        SCALE.store(factor.to_bits(), Ordering::SeqCst);
        true
    }
    fn param_value_changed(&self, _: &str, _: f32) {}
    fn param_modulation_changed(&self, _: &str, _: f32) {}
    fn param_values_changed(&self) {}
}

#[derive(Default)]
struct ViewTestPlugin(GlicolVst3);
impl Plugin for ViewTestPlugin {
    const NAME: &'static str = "View lifecycle test";
    const VENDOR: &'static str = GlicolVst3::VENDOR;
    const URL: &'static str = GlicolVst3::URL;
    const EMAIL: &'static str = "";
    const VERSION: &'static str = GlicolVst3::VERSION;
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = GlicolVst3::AUDIO_IO_LAYOUTS;
    type SysExMessage = ();
    type BackgroundTask = ();
    fn params(&self) -> Arc<dyn Params> {
        self.0.params.clone()
    }
    fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(FixedScaleEditor))
    }
    fn process(
        &mut self,
        _: &mut Buffer,
        _: &mut AuxiliaryBuffers,
        _: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}
impl Vst3Plugin for ViewTestPlugin {
    const VST3_CLASS_ID: [u8; 16] = [0x7b; 16];
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = GlicolVst3::VST3_SUBCATEGORIES;
}

type RecordedRects = Arc<Mutex<Vec<(i32, i32, i32, i32)>>>;
struct HostFrame(RecordedRects);
impl Class for HostFrame {
    type Interfaces = (IPlugFrame,);
}
impl IPlugFrameTrait for HostFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, new_size: *mut ViewRect) -> i32 {
        let rect = unsafe { &mut *new_size };
        self.0
            .lock()
            .unwrap()
            .push((rect.left, rect.top, rect.right, rect.bottom));
        let view = unsafe { ComRef::from_raw(view) }.unwrap();
        let mut observed: ViewRect = unsafe { std::mem::zeroed() };
        // Real hosts synchronously reenter these methods. A held editor mutex
        // would deadlock here, and mismatched renderer/host dimensions fail onSize.
        assert_eq!(unsafe { view.getSize(&mut observed) }, kResultOk);
        assert_eq!((observed.right, observed.bottom), (rect.right, rect.bottom));
        assert_eq!(unsafe { view.onSize(rect) }, kResultOk);
        kResultOk
    }
}

#[test]
fn late_host_dpi_recreates_child_and_requests_matching_origin_zero_size() {
    for before_attach in [false, true] {
        OPEN.store(false, Ordering::SeqCst);
        SPAWNS.store(0, Ordering::SeqCst);
        DROPS.store(0, Ordering::SeqCst);
        SCALE.store(1.0_f32.to_bits(), Ordering::SeqCst);
        let wrapper = ComWrapper::new(nice_plug::wrapper::vst3::Wrapper::<ViewTestPlugin>::new());
        let controller = wrapper.to_com_ptr::<IEditController>().unwrap();
        let requests: RecordedRects = Arc::new(Mutex::new(Vec::new()));
        let frame = ComWrapper::new(HostFrame(requests.clone()))
            .to_com_ptr::<IPlugFrame>()
            .unwrap();
        unsafe {
            assert_eq!(controller.initialize(std::ptr::null_mut()), kResultOk);
            let view =
                vst3::ComPtr::from_raw(controller.createView(b"editor\0".as_ptr().cast())).unwrap();
            let dpi = view.cast::<IPlugViewContentScaleSupport>().unwrap();
            assert_eq!(view.setFrame(frame.as_ptr()), kResultOk);
            if before_attach {
                assert_eq!(dpi.setContentScaleFactor(1.5), kResultOk);
            }
            assert_eq!(
                view.attached(1_usize as *mut _, b"HWND\0".as_ptr().cast()),
                kResultOk
            );
            assert_eq!(SPAWNS.load(Ordering::SeqCst), 1);
            if !before_attach {
                assert_eq!(dpi.setContentScaleFactor(1.5), kResultOk);
                assert_eq!(SPAWNS.load(Ordering::SeqCst), 2);
                assert_eq!(requests.lock().unwrap().last(), Some(&(0, 0, 840, 600)));
            }
            assert_eq!(dpi.setContentScaleFactor(2.0), kResultOk);
            assert_eq!(requests.lock().unwrap().last(), Some(&(0, 0, 900, 600)));
            let spawns = SPAWNS.load(Ordering::SeqCst);
            let request_count = requests.lock().unwrap().len();
            assert_eq!(dpi.setContentScaleFactor(2.0), kResultOk);
            assert_eq!(SPAWNS.load(Ordering::SeqCst), spawns);
            assert_eq!(requests.lock().unwrap().len(), request_count);
            assert_eq!(dpi.setContentScaleFactor(f32::NAN), kInvalidArgument);
            assert_eq!(dpi.setContentScaleFactor(0.0), kInvalidArgument);
            assert_eq!(SPAWNS.load(Ordering::SeqCst), spawns);
            // Unsupported positive scales leave the old scale and recreate a
            // usable child, rather than losing the host's editor.
            assert_eq!(dpi.setContentScaleFactor(4.0), kResultFalse);
            assert!(OPEN.load(Ordering::SeqCst));
            assert_eq!(f32::from_bits(SCALE.load(Ordering::SeqCst)), 2.0);
            assert_eq!(view.removed(), kResultOk);
            assert!(!OPEN.load(Ordering::SeqCst));
            let dropped = DROPS.load(Ordering::SeqCst);
            assert_eq!(SPAWNS.load(Ordering::SeqCst), dropped);
            // Reopen the same view at the already-negotiated scale.
            assert_eq!(
                view.attached(1_usize as *mut _, b"HWND\0".as_ptr().cast()),
                kResultOk
            );
            let mut rect: ViewRect = std::mem::zeroed();
            assert_eq!(view.getSize(&mut rect), kResultOk);
            assert_eq!(
                (rect.left, rect.top, rect.right, rect.bottom),
                (0, 0, 900, 600)
            );
            assert_eq!(view.removed(), kResultOk);
            assert_eq!(SPAWNS.load(Ordering::SeqCst), DROPS.load(Ordering::SeqCst));
            assert_eq!(view.setFrame(std::ptr::null_mut()), kResultOk);
            // A new view resets its own factor to 1.0, but the shared editor
            // still has 2.0. Do not mistake the wrapper's default for the
            // actual renderer's scale and silently skip renegotiation.
            let fresh =
                vst3::ComPtr::from_raw(controller.createView(b"editor\0".as_ptr().cast())).unwrap();
            assert_eq!(fresh.setFrame(frame.as_ptr()), kResultOk);
            assert_eq!(
                fresh.attached(1_usize as *mut _, b"HWND\0".as_ptr().cast()),
                kResultOk
            );
            let fresh_dpi = fresh.cast::<IPlugViewContentScaleSupport>().unwrap();
            assert_eq!(fresh_dpi.setContentScaleFactor(1.0), kResultOk);
            assert_eq!(SCALE.load(Ordering::SeqCst), 1.0_f32.to_bits());
            assert_eq!(requests.lock().unwrap().last(), Some(&(0, 0, 560, 420)));
            assert_eq!(fresh.removed(), kResultOk);
            assert_eq!(fresh.setFrame(std::ptr::null_mut()), kResultOk);
            assert_eq!(controller.terminate(), kResultOk);
        }
    }
}
