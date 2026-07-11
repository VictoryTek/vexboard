#[cfg(all(unix, feature = "pam-auth"))]
mod pam_impl {
    use pam_sys::{
        PamConversation, PamFlag, PamHandle, PamMessageStyle, PamResponse, PamReturnCode,
    };
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;

    // PAM conversation callback: responds to password prompts with the supplied password.
    // The response array and each resp string are allocated with libc malloc so the
    // PAM library can free() them as required by the PAM specification.
    extern "C" fn conversation(
        num_msg: c_int,
        msg: *mut *mut pam_sys::PamMessage,
        resp: *mut *mut PamResponse,
        appdata_ptr: *mut c_void,
    ) -> c_int {
        if num_msg <= 0 || msg.is_null() || resp.is_null() {
            return PamReturnCode::CONV_ERR as c_int;
        }

        let num = num_msg as usize;

        // Allocate the response array with libc calloc so PAM can free() it.
        let responses =
            unsafe { libc::calloc(num, std::mem::size_of::<PamResponse>()) as *mut PamResponse };
        if responses.is_null() {
            return PamReturnCode::BUF_ERR as c_int;
        }

        let password_ptr = appdata_ptr as *const c_char;

        for i in 0..num {
            // SAFETY: PAM guarantees msg[i] is valid for i in 0..num_msg.
            let msg_style = unsafe { (**msg.add(i)).msg_style };
            let needs_response = msg_style == PamMessageStyle::PROMPT_ECHO_OFF as c_int
                || msg_style == PamMessageStyle::PROMPT_ECHO_ON as c_int;

            if needs_response {
                // SAFETY: password_ptr is the interior pointer of a live CString.
                let (len, buf) = unsafe {
                    let len = libc::strlen(password_ptr);
                    let buf = libc::malloc(len + 1) as *mut c_char;
                    (len, buf)
                };

                if buf.is_null() {
                    // Free any already-allocated response strings and the array itself.
                    unsafe {
                        for j in 0..i {
                            let r = &*responses.add(j);
                            if !r.resp.is_null() {
                                libc::free(r.resp as *mut c_void);
                            }
                        }
                        libc::free(responses as *mut c_void);
                    }
                    return PamReturnCode::BUF_ERR as c_int;
                }

                unsafe {
                    libc::memcpy(buf as *mut c_void, password_ptr as *const c_void, len + 1);
                    (*responses.add(i)).resp = buf;
                }
            }
        }

        unsafe { *resp = responses };
        PamReturnCode::SUCCESS as c_int
    }

    pub fn authenticate_pam(username: &str, password: &str) -> bool {
        let password_c = match CString::new(password) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let conv = PamConversation {
            conv: Some(conversation),
            data_ptr: password_c.as_ptr() as *mut c_void,
        };

        let mut handle: *mut PamHandle = ptr::null_mut();

        let ret = pam_sys::start("vexboard", Some(username), &conv, &mut handle);
        if ret != PamReturnCode::SUCCESS {
            return false;
        }

        // SAFETY: pam_start succeeded, so handle is non-null and valid.
        let handle_ref = unsafe { &mut *handle };

        let ret = pam_sys::authenticate(handle_ref, PamFlag::NONE);
        if ret != PamReturnCode::SUCCESS {
            pam_sys::end(handle_ref, ret);
            return false;
        }

        let acct_ret = pam_sys::acct_mgmt(handle_ref, PamFlag::NONE);
        let success = acct_ret == PamReturnCode::SUCCESS;

        pam_sys::end(handle_ref, acct_ret);

        success
    }
}

#[cfg(all(unix, feature = "pam-auth"))]
pub use pam_impl::authenticate_pam;
