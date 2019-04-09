use crate::errors::Error;
use image::png::PNGEncoder;
use image::{Luma, Pixel};
use qrcode::{EcLevel, QrCode};

pub fn as_png(s: &str) -> Result<Vec<u8>, Error> {
    let qrcode =
        QrCode::with_error_correction_level(s, EcLevel::L).map_err(|e| Error::General(s!(e)))?;
    let png = qrcode.render::<Luma<u8>>().module_dimensions(4, 4).build();
    let mut buf: Vec<u8> = Vec::new();
    PNGEncoder::new(&mut buf)
        .encode(&png, png.width(), png.height(), Luma::<u8>::color_type())
        .map_err(|e| Error::General(format!("Cannot write PNG file: {}", e)))?;
    Ok(buf)
}
