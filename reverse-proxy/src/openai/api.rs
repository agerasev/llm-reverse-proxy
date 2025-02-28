use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message<'a> {
    pub role: Cow<'a, str>,
    pub content: Cow<'a, str>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request<'a> {
    pub model: Cow<'a, str>,
    pub messages: Vec<Message<'a>>,
    pub stream: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice<'a> {
    pub message: Message<'a>,
    pub index: Option<usize>,
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response<'a> {
    pub choices: SmallVec<[Choice<'a>; 1]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delta<'a> {
    pub content: Option<Cow<'a, str>>,
    pub role: Option<Cow<'a, str>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamChoice<'a> {
    pub delta: Delta<'a>,
    pub index: Option<usize>,
    pub finish_reason: Option<Cow<'a, str>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseStreamChunk<'a> {
    pub choices: SmallVec<[StreamChoice<'a>; 1]>,
}
