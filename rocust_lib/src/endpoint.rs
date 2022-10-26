use parking_lot::RwLock;
use crate::{Results, HasResults};
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::GET => write!(f, "GET"),
            Method::POST => write!(f, "POST"),
            Method::PUT => write!(f, "PUT"),
            Method::DELETE => write!(f, "DELETE"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EndPoint {
    pub method: Method,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub params: Option<Vec<(String, String)>>,
    pub body: Option<String>,
    pub results: Arc<RwLock<Results>>, //ENDPOINT RESULTS
}

impl Serialize for EndPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("EndPoint", 6)?;
        state.serialize_field("method", &self.method)?;
        state.serialize_field("url", &self.url)?;
        state.serialize_field("headers", &self.headers)?;
        state.serialize_field("params", &self.params)?;
        state.serialize_field("body", &self.body)?;
        state.serialize_field("results", &*self.results.read())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for EndPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EndPointVisitor;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Method,
            Url,
            Headers,
            Params,
            Body,
            Results,
        }
        impl<'de> Visitor<'de> for EndPointVisitor {
            type Value = EndPoint;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct EndPoint")
            }

            fn visit_map<V>(self, mut map: V) -> Result<EndPoint, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut method: Option<Method> = None;
                let mut url: Option<String> = None;
                let mut headers: Option<Option<HashMap<String, String>>> = None;
                let mut params: Option<Option<Vec<(String, String)>>> = None;
                let mut body: Option<Option<String>> = None;
                let mut results: Option<Results> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Method => {
                            if method.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            method = Some(map.next_value()?);
                        }
                        Field::Url => {
                            if url.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            url = Some(map.next_value()?);
                        }
                        Field::Headers => {
                            if headers.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            headers = Some(map.next_value()?);
                        }
                        Field::Params => {
                            if params.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            params = Some(map.next_value()?);
                        }
                        Field::Body => {
                            if body.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            body = Some(map.next_value()?);
                        }
                        Field::Results => {
                            if results.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            results = Some(map.next_value()?);
                        }
                    }
                }
                let method = method.ok_or_else(|| serde::de::Error::missing_field("method"))?;
                let url = url.ok_or_else(|| serde::de::Error::missing_field("url"))?;
                let headers = headers.ok_or_else(|| serde::de::Error::missing_field("headers"))?;
                let params = params.ok_or_else(|| serde::de::Error::missing_field("params"))?;
                let body = body.ok_or_else(|| serde::de::Error::missing_field("body"))?;
                let results = results.ok_or_else(|| serde::de::Error::missing_field("results"))?;

                Ok(EndPoint {
                    method,
                    url,
                    params,
                    body,
                    results: Arc::new(RwLock::new(results)),
                    headers,
                })
            }
        }
        const FIELDS: &'static [&'static str] =
            &["method", "url", "headers", "params", "body", "results"];
        deserializer.deserialize_struct("EndPoint", &FIELDS, EndPointVisitor)
    }
}

impl EndPoint {
    fn new(
        method: Method,
        url: String,
        headers: Option<HashMap<String, String>>,
        params: Option<Vec<(String, String)>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint {
            method,
            url,
            params,
            body,
            results: Arc::new(RwLock::new(Results::new())),
            headers,
        }
    }

    pub fn new_get(
        url: String,
        headers: Option<HashMap<String, String>>,
        params: Option<Vec<(String, String)>>,
    ) -> EndPoint {
        EndPoint::new(Method::GET, url, headers, params, None)
    }

    pub fn new_post(
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint::new(Method::POST, url, headers, None, body)
    }

    pub fn new_put(
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint::new(Method::PUT, url, headers, None, body)
    }

    pub fn new_delete(url: String, headers: Option<HashMap<String, String>>) -> EndPoint {
        EndPoint::new(Method::DELETE, url, headers, None, None)
    }

    pub fn get_method(&self) -> &Method {
        &self.method
    }

    pub fn get_url(&self) -> &String {
        &self.url
    }

    pub fn get_results(&self) -> &Arc<RwLock<Results>> {
        &self.results
    }

    pub fn get_params(&self) -> &Option<Vec<(String, String)>> {
        &self.params
    }

    pub fn get_body(&self) -> &Option<String> {
        &self.body
    }
}

impl fmt::Display for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Method [{}] | Url [{}] | Results [{}]",
            self.method,
            self.url,
            self.results.read()
        )
    }
}

impl HasResults for EndPoint {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn add_failed(&self) {
        self.results.write().add_failed();
    }

    fn add_connection_error(&self) {
        self.results.write().add_connection_error();
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, elapsed: &Duration) {
        self.results.write().calculate_requests_per_second(elapsed);
    }

    fn calculate_failed_requests_per_second(&self, elapsed: &Duration) {
        self.results
            .write()
            .calculate_failed_requests_per_second(elapsed);
    }

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
    
    fn clone_results(&self) -> Results {
        self.results.read().clone()
    }

}
