use crate::{shinkai_message::shinkai_message::{ShinkaiMessage, Body, ExternalMetadata}, shinkai_utils::encryption::{self, EncryptionMethod}};
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShinkaiMessageWrapper {
    inner: ShinkaiMessage,
}

#[wasm_bindgen]
impl ShinkaiMessageWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new(body: &JsValue, external_metadata: &JsValue, encryption: EncryptionMethod) -> Result<ShinkaiMessageWrapper, JsValue> {
        let body = Body::from_jsvalue(body).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let external_metadata = ExternalMetadata::from_jsvalue(external_metadata).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let shinkai_message = ShinkaiMessage::new(Some(body), Some(external_metadata), encryption);

        Ok(ShinkaiMessageWrapper {
            inner: shinkai_message,
        })
    }

    #[wasm_bindgen(method, getter)]
    pub fn body(&self) -> Result<JsValue, JsValue> {
        self.inner.body.clone().unwrap().to_jsvalue().map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(method, setter)]
    pub fn set_body(&mut self, body: JsValue) -> Result<(), JsValue> {
        let body = Body::from_jsvalue(&body).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.body = Some(body);
        Ok(())
    }

    #[wasm_bindgen(method, getter)]
    pub fn external_metadata(&self) -> Result<JsValue, JsValue> {
        self.inner.external_metadata.clone().unwrap().to_jsvalue().map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(method, setter)]
    pub fn set_external_metadata(&mut self, external_metadata: JsValue) -> Result<(), JsValue> {
        let external_metadata = ExternalMetadata::from_jsvalue(&external_metadata).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.external_metadata = Some(external_metadata);
        Ok(())
    }

    #[wasm_bindgen(method, getter)]
    pub fn encryption(&self) -> String {
        log::debug!("encryption: {:?}", self);
        self.inner.encryption.as_str().to_owned()
    }

    #[wasm_bindgen(method, setter)]
    pub fn set_encryption(&mut self, encryption: String) {
        log::debug!("set encryption: {:?}", encryption);
        self.inner.encryption = EncryptionMethod::from_str(&encryption);
    }

    #[wasm_bindgen(method)]
    pub fn to_jsvalue(&self) -> Result<JsValue, JsValue> {
        self.inner.to_jsvalue()
    }

    #[wasm_bindgen(js_name = fromJsValue)]
    pub fn from_jsvalue(j: &JsValue) -> Result<ShinkaiMessageWrapper, JsValue> {
        let inner = ShinkaiMessage::from_jsvalue(j)?;
        Ok(ShinkaiMessageWrapper { inner })
    } 
}
