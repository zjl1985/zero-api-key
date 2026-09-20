use anyhow::{Result, anyhow, bail};
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};

const LA_ERROR_USER_CANCEL: isize = -2;

pub fn authenticate(reason: &str) -> Result<()> {
    unsafe {
        let context = LAContext::new();
        context
            .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication)
            .map_err(|e| anyhow!("此设备无法进行机主验证：{}", e.localizedDescription()))?;
        let (tx, rx) = std::sync::mpsc::channel();
        let block = RcBlock::new(move |success: Bool, error: *mut NSError| {
            let result = if success.as_bool() {
                Ok(())
            } else if error.is_null() {
                Err("指纹验证失败".to_string())
            } else if (*error).code() == LA_ERROR_USER_CANCEL {
                Err("已取消指纹验证，保险库未解锁".to_string())
            } else {
                Err(format!("指纹验证失败：{}", (*error).localizedDescription()))
            };
            let _ = tx.send(result);
        });
        context.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthentication,
            &NSString::from_str(reason),
            &block,
        );
        match rx.recv() {
            Ok(result) => result.map_err(|e| anyhow!(e)),
            Err(_) => bail!("指纹验证通道异常"),
        }
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
