//! Media: avatar pictures.

use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{MxcUri, OwnedMxcUri, UInt, events::room::MediaSource, media::Method},
};

use crate::{Error, Result, Session};

impl Session {
    /// A thumbnail of `mxc` cropped to about `size`×`size` pixels, as encoded
    /// image bytes (PNG, JPEG, ...). The server may return a bigger one.
    /// Downloads are cached in the store.
    pub async fn thumbnail(&self, mxc: &MxcUri, size: u32) -> Result<Vec<u8>> {
        let size = UInt::from(size);
        let request = MediaRequestParameters {
            source: MediaSource::Plain(mxc.to_owned()),
            format: MediaFormat::Thumbnail(MediaThumbnailSettings::with_method(
                Method::Crop,
                size,
                size,
            )),
        };
        Ok(self
            .client
            .media()
            .get_media_content(&request, true)
            .await?)
    }

    /// Upload `data` (of MIME type `content_type`, e.g. `image/png`) and make
    /// it our avatar.
    pub async fn set_avatar(&self, content_type: &str, data: Vec<u8>) -> Result<OwnedMxcUri> {
        let content_type = content_type
            .parse()
            .map_err(|_| Error::InvalidMediaType(content_type.to_owned()))?;
        Ok(self
            .client
            .account()
            .upload_avatar(&content_type, data)
            .await?)
    }
}
