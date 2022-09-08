use rocust_lib::{EndPoint, Method, Test};
use poem::{
    get, handler, listener::TcpListener, middleware::AddData, web::Data, EndpointExt, Route, Server,
};

#[handler]
fn start_test(test: Data<&Test>) -> String {
    let mut test = test.clone();
    tokio::spawn(async move {
        test.run().await;
    });
    format!("Ok")
}

#[handler]
fn stop_test(test: Data<&Test>) -> String {
    test.stop();
    format!("Ok")
}

#[handler]
fn index() -> String {
    format!("Ok")
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let test = Test::new(
        10,
        Some(10),
        5,
        "https://google.com".to_string(),
        vec![
            EndPoint::new(Method::GET, "/".to_string(), None),
            EndPoint::new(Method::GET, "/get".to_string(), None),
            EndPoint::new(Method::POST, "/post".to_string(), None),
            EndPoint::new(Method::PUT, "/put".to_string(), None),
            EndPoint::new(Method::DELETE, "/delete".to_string(), None),
        ],
        None,
    );

    let app = Route::new()
        .at("/", get(index))
        .at("/start", get(start_test))
        .at("/stop", get(stop_test))
        .with(AddData::new(test));
    Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(app)
        .await
}

