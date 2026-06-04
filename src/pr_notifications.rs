use anyhow::{Context, Result, anyhow};
use pullbell::model::PullRequestItem;
use std::ffi::CString;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pullbell_install_notification_delegate();
    fn pullbell_send_pr_notification(
        title: *const std::ffi::c_char,
        subtitle: *const std::ffi::c_char,
        message: *const std::ffi::c_char,
        url: *const std::ffi::c_char,
        icon_path: *const std::ffi::c_char,
    ) -> bool;
}

pub(super) fn install_delegate() {
    #[cfg(target_os = "macos")]
    unsafe {
        pullbell_install_notification_delegate();
    }
}

pub(super) fn send(item: &PullRequestItem) -> Result<()> {
    let title = CString::new("Pullbell")?;
    let subtitle = CString::new(item.kind.label())?;
    let message = CString::new(item.display_title())
        .with_context(|| format!("building notification message for {}", item.id))?;
    let url = CString::new(item.url.as_str())
        .with_context(|| format!("building notification URL for {}", item.id))?;
    let icon_path = CString::new(crate::notification_icon_path().unwrap_or_default())?;

    send_raw(&title, &subtitle, &message, &url, &icon_path)
}

fn send_raw(
    title: &CString,
    subtitle: &CString,
    message: &CString,
    url: &CString,
    icon_path: &CString,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let delivered = unsafe {
            pullbell_send_pr_notification(
                title.as_ptr(),
                subtitle.as_ptr(),
                message.as_ptr(),
                url.as_ptr(),
                icon_path.as_ptr(),
            )
        };

        if delivered {
            Ok(())
        } else {
            Err(anyhow!("macOS rejected the PR notification payload"))
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, subtitle, message, url, icon_path);
        Ok(())
    }
}
