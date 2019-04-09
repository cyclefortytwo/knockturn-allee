use crate::errors::Error;
use crate::qrcode;
use consistenttime::ct_u8_slice_eq;

pub struct Totp {
    merchant: String,
    token: String,
}

impl Totp {
    pub fn new(merchant: String, token: String) -> Self {
        Totp { merchant, token }
    }

    pub fn get_png(&self) -> Result<Vec<u8>, Error> {
        let code_str = format!(
            "otpauth://totp/Knockturn:{}?secret={}&issuer=Knockturn",
            self.merchant, self.token
        );
        qrcode::as_png(&code_str)
    }

    pub fn generate(&self) -> Result<String, Error> {
        let totp = boringauth::oath::TOTPBuilder::new()
            .base32_key(&self.token)
            .finalize()
            .map_err(|e| Error::General(format!("Got error code from boringauth {:?}", e)))?;
        Ok(totp.generate())
    }

    pub fn check(&self, code: &str) -> Result<bool, Error> {
        let corrent_code = self.generate()?;
        Ok(ct_u8_slice_eq(corrent_code.as_bytes(), code.as_bytes()))
    }
}
