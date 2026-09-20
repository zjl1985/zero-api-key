use std::time::Instant;

use anyhow::{Result, anyhow};
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};

const LA_ERROR_USER_CANCEL: isize = -2;
const LA_ERROR_BIOMETRY_NOT_AVAILABLE: isize = -6;
const LA_ERROR_BIOMETRY_NOT_ENROLLED: isize = -7;
const LA_ERROR_BIOMETRY_LOCKOUT: isize = -8;

pub fn authenticate(reason: &str) -> Result<()> {
    unsafe {
        let context = LAContext::new();
        context.setTouchIDAuthenticationAllowableReuseDuration(0.0);
        let first = evaluate(&context, LAPolicy::DeviceOwnerAuthenticationWithBiometrics, reason);
        let result = match first {
            Err((code, msg))
                if matches!(
                    code,
                    LA_ERROR_BIOMETRY_NOT_AVAILABLE
                        | LA_ERROR_BIOMETRY_NOT_ENROLLED
                        | LA_ERROR_BIOMETRY_LOCKOUT
                ) =>
            {
                eprintln!("[touch_id] 生物识别不可用（LAError {code}: {msg}），回落到设备密码验证");
                evaluate(&context, LAPolicy::DeviceOwnerAuthentication, reason)
                    .map_err(|(_, msg)| anyhow!(msg))
            }
            Err((_, msg)) => Err(anyhow!(msg)),
            Ok(()) => Ok(()),
        };
        context.invalidate();
        result
    }
}

unsafe fn evaluate(
    context: &LAContext,
    policy: LAPolicy,
    reason: &str,
) -> Result<(), (isize, String)> {
    unsafe {
        if let Err(e) = context.canEvaluatePolicy_error(policy) {
            return Err((e.code(), format!("{}", e.localizedDescription())));
        }
        eprintln!("[touch_id] 等待指纹验证…");
        let start = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let block = RcBlock::new(move |success: Bool, error: *mut NSError| {
            let result = if success.as_bool() {
                Ok(())
            } else if error.is_null() {
                Err((0, "验证失败（无错误信息）".to_string()))
            } else {
                let code = (*error).code();
                let msg = if code == LA_ERROR_USER_CANCEL {
                    "已取消指纹验证，保险库未解锁".to_string()
                } else {
                    format!("验证失败（LAError {code}）：{}", (*error).localizedDescription())
                };
                Err((code, msg))
            };
            let _ = tx.send(result);
        });
        context.evaluatePolicy_localizedReason_reply(policy, &NSString::from_str(reason), &block);
        let result = rx.recv().unwrap_or_else(|_| Err((0, "指纹验证通道异常".to_string())));
        match &result {
            Ok(()) => eprintln!("[touch_id] 验证通过，耗时 {:.1}s", start.elapsed().as_secs_f64()),
            Err((code, msg)) => eprintln!(
                "[touch_id] 验证未通过（LAError {code}: {msg}），耗时 {:.1}s",
                start.elapsed().as_secs_f64()
            ),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要真机 Touch ID，手动运行：cargo test touch_id -- --ignored"]
    fn manual_authenticate() {
        authenticate("zero-api-key 测试").unwrap();
    }
}
