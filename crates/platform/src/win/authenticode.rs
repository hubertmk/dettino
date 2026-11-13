//! Authenticode check for installers Dettino downloads and runs: the file must carry
//! a signature that chains to a trusted root (`WinVerifyTrust`), and the caller gets
//! the signer's name to compare with the expected publisher.

use std::ffi::c_void;
use std::path::Path;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{
    CertCloseStore, CertFindCertificateInStore, CertFreeCertificateContext, CertGetNameStringW, CryptMsgClose, CryptMsgGetParam,
    CryptQueryObject, CERT_FIND_SUBJECT_CERT, CERT_INFO, CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
    CERT_QUERY_CONTENT_TYPE, CERT_QUERY_ENCODING_TYPE, CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_FORMAT_TYPE, CERT_QUERY_OBJECT_FILE,
    CMSG_SIGNER_INFO, CMSG_SIGNER_INFO_PARAM, HCERTSTORE, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows::Win32::Security::WinTrust::{
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
    WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

/// Check that `path` has a valid embedded Authenticode signature and return the
/// signer's display name (the certificate's simple name, e.g. "Ollama").
pub fn verify_authenticode(path: &Path) -> Result<String, String> {
    let wide = super::wide(&path.to_string_lossy());
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(wide.as_ptr()),
            hFile: HANDLE::default(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut data = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            // No network: a machine without internet must still be able to verify.
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        };
        let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut c_void);
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut c_void);
        if status != 0 {
            return Err(format!("no valid Authenticode signature (WinVerifyTrust 0x{:08x})", status as u32));
        }
    }
    signer_name(&wide)
}

/// The simple display name of the certificate that signed the file.
fn signer_name(wide: &[u16]) -> Result<String, String> {
    unsafe {
        let mut encoding = CERT_QUERY_ENCODING_TYPE::default();
        let mut content = CERT_QUERY_CONTENT_TYPE::default();
        let mut format = CERT_QUERY_FORMAT_TYPE::default();
        let mut store = HCERTSTORE::default();
        let mut msg: *mut c_void = std::ptr::null_mut();
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr() as *const c_void,
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            Some(&mut encoding),
            Some(&mut content),
            Some(&mut format),
            Some(&mut store),
            Some(&mut msg),
            None,
        )
        .map_err(|e| format!("cannot read the signature: {e}"))?;
        let result = (|| -> Result<String, String> {
            let mut size = 0u32;
            CryptMsgGetParam(msg as *const c_void, CMSG_SIGNER_INFO_PARAM, 0, None, &mut size).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; size as usize];
            CryptMsgGetParam(msg as *const c_void, CMSG_SIGNER_INFO_PARAM, 0, Some(buf.as_mut_ptr() as *mut c_void), &mut size)
                .map_err(|e| e.to_string())?;
            let signer = &*(buf.as_ptr() as *const CMSG_SIGNER_INFO);
            let info = CERT_INFO { Issuer: signer.Issuer, SerialNumber: signer.SerialNumber, ..Default::default() };
            let ctx = CertFindCertificateInStore(
                store,
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                0,
                CERT_FIND_SUBJECT_CERT,
                Some(&info as *const CERT_INFO as *const c_void),
                None,
            );
            if ctx.is_null() {
                return Err("the signer's certificate is not in the signature".into());
            }
            let mut name = vec![0u16; 512];
            let n = CertGetNameStringW(ctx, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut name));
            let _ = CertFreeCertificateContext(Some(ctx));
            Ok(String::from_utf16_lossy(&name[..(n as usize).saturating_sub(1)]))
        })();
        let _ = CryptMsgClose(Some(msg as *const c_void));
        let _ = CertCloseStore(Some(store), 0);
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsigned_files_are_rejected() {
        let me = std::env::current_exe().unwrap();
        assert!(super::verify_authenticode(&me).is_err(), "the test binary carries no signature");
    }

    /// On a PC that has Ollama installed, its own app must pass and name Ollama.
    #[test]
    fn signed_ollama_app_is_accepted() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else { return };
        let app = std::path::PathBuf::from(local).join("Programs").join("Ollama").join("ollama app.exe");
        if !app.is_file() {
            return;
        }
        let signer = super::verify_authenticode(&app).expect("Ollama's app is Authenticode-signed");
        assert!(signer.to_ascii_lowercase().contains("ollama"), "signer: {signer}");
    }
}
