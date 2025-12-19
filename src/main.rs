use dotenvy::dotenv;
use oxideq::app;
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    dotenv().ok();
    println!("Hello, world!");

    let password = env::var("OXIDEQ_PASSWORD").expect("OXIDEQ_PASSWORD must be set");

    let cleanup_interval = env::var("OXIDEQ_CLEANUP_INTERVAL")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .expect("OXIDEQ_CLEANUP_INTERVAL must be a number");

    let task_timeout = env::var("OXIDEQ_TASK_TIMEOUT")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .expect("OXIDEQ_TASK_TIMEOUT must be a number");

    let app = app(password, cleanup_interval, task_timeout);

    let port_str = env::var("OXIDEQ_PORT").unwrap_or_else(|_| "8540".to_string());
    let port: u16 = port_str.parse().expect("OXIDEQ_PORT must be a number");
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("Server running on http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
