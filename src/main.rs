use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortCreateRunLoopSource;
use core_foundation::runloop::{
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRun, kCFRunLoopCommonModes,
};

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct State {
    is_active: AtomicBool,
    l_shift: AtomicBool,
    r_shift: AtomicBool,
    start_time: Mutex<Option<Instant>>,
}

fn main() {
    let state = Arc::new(State {
        is_active: AtomicBool::new(false),
        l_shift: AtomicBool::new(false),
        r_shift: AtomicBool::new(false),
        start_time: Mutex::new(None),
    });

    let state_clone = Arc::clone(&state);

    let tap = CGEventTap::new(
        CGEventTapLocation::HID, // 미디어 키 등 로우레벨 이벤트를 위해 HID 사용
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            // 미디어 키 등은 시스템 정의 이벤트로 올 수 있으나
            // core-graphics enum에 없는 경우가 많아 최대한 포괄적으로 잡습니다.
        ],
        move |_proxy, event_type, event| {
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);

            // 시프트 및 엔터 키 코드 정의
            const L_SHIFT: i64 = 56;
            const R_SHIFT: i64 = 60;
            const ENTER: i64 = 36;

            let is_keydown = matches!(event_type, CGEventType::KeyDown);
            // let is_keyup = matches!(event_type, CGEventType::KeyUp);
            let is_flags_changed = matches!(event_type, CGEventType::FlagsChanged);

            // 시프트 상태 추적
            if is_flags_changed {
                let flags = event.get_flags();
                let is_shift_down = flags.contains(CGEventFlags::CGEventFlagShift);

                if keycode == L_SHIFT {
                    state_clone.l_shift.store(is_shift_down, Ordering::SeqCst);
                } else if keycode == R_SHIFT {
                    state_clone.r_shift.store(is_shift_down, Ordering::SeqCst);
                }
            }

            // 토글 로직 체크
            if is_keydown
                && keycode == ENTER
                && state_clone.l_shift.load(Ordering::SeqCst)
                && state_clone.r_shift.load(Ordering::SeqCst)
            {
                let currently_active = state_clone.is_active.load(Ordering::SeqCst);
                let new_active = !currently_active;

                state_clone.is_active.store(new_active, Ordering::SeqCst);

                let mut start_time = state_clone.start_time.lock().unwrap();
                if new_active {
                    println!("🧹 Key Clean Mode: ENABLED (1 min)");
                    *start_time = Some(Instant::now());
                } else {
                    println!("✅ Key Clean Mode: DISABLED");
                    *start_time = None;
                }
                return CallbackResult::Drop;
            }

            // 현재 차단 모드인지 확인 및 시간 초과 체크
            let mut is_active = state_clone.is_active.load(Ordering::SeqCst);
            if is_active {
                let mut start_time_lock = state_clone.start_time.lock().unwrap();
                if let Some(start) = *start_time_lock
                    && start.elapsed() >= Duration::from_secs(60)
                {
                    println!("⏰ Timeout: Key Clean Mode DISABLED");
                    state_clone.is_active.store(false, Ordering::SeqCst);
                    *start_time_lock = None;
                    is_active = false;
                }
            }

            // 차단 모드일 때의 처리
            if is_active {
                // 토글을 위한 특수 처리:
                // 1. 시프트 상태 변경(FlagsChanged)은 허용하되 상태만 업데이트하고 전파는 막아야 할 수도 있음
                // 2. 여기서는 시프트/엔터 관련 이벤트만 통과시키고 나머지는 모두 Drop

                /* let is_toggle_key = keycode == L_SHIFT || keycode == R_SHIFT || keycode == ENTER;

                if is_toggle_key {
                    // 토글 키들은 시스템에 전달되어야 정상적으로 상태 추적이 가능할 때가 있음
                    // (특히 시스템이 이 키들을 먹어버리면 곤란하므로)
                    // return CallbackResult::Keep;
                    return CallbackResult::Drop;
                } */

                // 그 외 모든 이벤트(F-열, 문자열 등) 차단
                return CallbackResult::Drop;
            }

            CallbackResult::Keep
        },
    )
    .expect("failed to create event tap");

    let run_loop_source = unsafe {
        CFMachPortCreateRunLoopSource(
            std::ptr::null_mut(),
            tap.mach_port().as_concrete_TypeRef(),
            0,
        )
    };

    unsafe {
        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            run_loop_source,
            kCFRunLoopCommonModes,
        );
    }

    tap.enable();

    println!("Listening... (L-Shift + R-Shift + Enter to toggle Clean Mode)");

    unsafe {
        CFRunLoopRun();
    }
}
