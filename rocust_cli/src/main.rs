use rocust_lib::{test::user::User, EndPoint, Master, Runnable, Test, Worker};
use std::{process::exit, time::Duration};

#[tokio::main(flavor = "multi_thread", worker_threads = 1000)]
async fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");

    let mut test = Test::new(
        String::from("test1"),
        21,
        Some(10),
        (1, 10),
        "https://google.com".to_string(),
        vec![
            EndPoint::new_get(
                "/".to_string(),
                None,
                Some(vec![(String::from("id"), String::from("6"))]),
            ),
            EndPoint::new_get("/get".to_string(), None, None),
            EndPoint::new_post(
                "/post".to_string(),
                None,
                Some(String::from("this is body")),
            ),
            EndPoint::new_put("/put".to_string(), None, None),
            EndPoint::new_delete("/delete".to_string(), None),
        ],
        None,
        format!("log/{}.log", "test1"),
        false,
        false,
    );

    let mut master = Master::new(
        String::from("Master"),
        2,
        test.clone(),
        String::from("127.0.0.1:3000"),
        String::from("log/master.log"),
        true,
        true,
    );
    let mut worker = Worker::new(
        String::from("Worker"),
        String::from("ws://127.0.0.1:3000/ws"),
        String::from("log/worker1.log"),
        false,
        false,
    );
    let mut worker2 = Worker::new(
        String::from("Worker 2"),
        String::from("ws://127.0.0.1:3000/ws"),
        String::from("log/worker2.log"),
        true,
        false,
    );
    let master_c = master.clone();

    // tokio::spawn(async move {
    //     tokio::time::sleep(Duration::from_secs(15)).await;
    //     println!("Master stopping");
    //     let _ = master_c.stop();
    // });

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = worker2.run().await;
        println!("worker2 finished");
    });

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = worker.run().await;
        //println!("worker1 finished: {:?}", worker);
    });

    let _ = master.run().await;

    //println!("Master finished: {:?}", master);
    // //println!("{:?}", master);
    tokio::time::sleep(Duration::from_secs(60)).await;
    exit(0);
    //tokio::time::sleep(Duration::from_secs(60)).await;

    // let test_handler = test.clone();
    // tokio::spawn(async move {
    //     println!("canceling user 1 in 50 seconds");
    //     tokio::time::sleep(Duration::from_secs(50)).await;
    //     println!("attempting cancel user 1");
    //     test_handler.stop_a_user(1).unwrap_or_default();
    // });

    let test_handler = test.clone();
    tokio::spawn(async move {
        println!("canceling user 0 in 3 seconds");
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("attempting cancel user 0");
        test_handler.stop_a_user(0).unwrap_or_default();
    });
    let test_handler = test.clone();
    tokio::spawn(async move {
        println!("canceling user 563 in 3 seconds");
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("attempting cancel user 563");
        test_handler.stop_a_user(563).unwrap_or_default();
    });

    // let test_handler = test.clone();
    // tokio::spawn(async move {
    //     loop {
    //         tokio::time::sleep(Duration::from_secs(1)).await;
    //         println!("STATUS: [{}]", test_handler.get_status().read());
    //     }
    // });

    //test.run().await;

    let test_handler = test.clone();
    tokio::spawn(async move {
        println!("canceling test in 5 seconds");
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("attempting cancel");
        //test_handler.stop();
        test_handler.stop();
    });

    test.run().await;

    println!("\n{}", test);
    // println!();
    // let endpoints = test.get_endpoints();
    // for endpoint in endpoints.iter() {
    //     println!("{}", endpoint);
    //     println!("------------------------------");
    // }
    println!();
    let users = test.get_users();
    for user in users.read().iter() {
        println!("{}\n", user);
        // for (endpoint_url, results) in user.get_endpoints().read().iter() {
        //     println!("\t[{}] | [{}]\n", endpoint_url, results);
        // }
        println!("------------------------------");
    }

    // println!("before: {:?}", test);
    // let j = serde_json::to_string(&test).unwrap();
    // let u: Test = serde_json::from_str(&j).unwrap();
    // println!("############################################################");
    // println!("after: {:?}", u);
    exit(0);
}
