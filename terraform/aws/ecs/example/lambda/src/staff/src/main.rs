use tracing::{event, Level};
use lambda_http::request::RequestContext;
use lambda_http::{run, service_fn, Error, IntoResponse, Request, RequestExt, Response};
use mysql::prelude::*;
use mysql::{params, Opts, Pool, PooledConn};
use serde::{Deserialize, Serialize};
use handlebars::Handlebars;
use std::collections::HashMap;
use std::env;

use handlebars::{ to_json };

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Staff {
    staff_id: i32,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Pagination {
    page: i32,
    next: i32,
    prev: i32
}

fn post_staff(event: Request, mut conn: PooledConn) -> Result<Response<String>, Error> {
    let payload = match event.payload::<Staff>() {
        Ok(Some(staff)) => staff,
        _ => panic!("Can't create staff from input")
    };

    let _ = conn.exec_drop(
        r"INSERT INTO staff (first_name, last_name, email, username, password, store_id, address_id)
        VALUES (:first_name, :last_name, :email, :username, :password, 1, 61)",
        params! {
            "first_name" => payload.first_name,
            "last_name" => payload.last_name,
            "email" => payload.email,
            "username" => payload.username,
            "password" => payload.password
        }
    )?;

    event!(Level::INFO, "Create STAFF - Last generated key: {}",
        conn.last_insert_id());

    let resp = Response::builder()
        .status(303)
        .header("Location", "/staff")
        .body(String::new())
        .map_err(Box::new)?;

    Ok(resp)
}

fn get_single_staff(mut conn: PooledConn, staff_id: String) -> Result<Vec<Staff>, Error> {
    event!(Level::INFO, "GET STAFF - by id: {}",
        staff_id);

    let staff = conn
        .exec_first(
            "SELECT staff_id, first_name, last_name, email, username, password FROM staff WHERE staff_id=:staff_id",
            params! {
                staff_id
            }
        ).map(|row|{
            row.map(|(staff_id, first_name, last_name, email, username, password)| Staff {
                staff_id,
                first_name,
                last_name,
                email,
                username,
                password
            })
        })?
        .unwrap();

    Ok(vec![staff])
}

fn get_list_staff(mut conn: PooledConn, page_num: i32) -> Result<Vec<Staff>, Error> {
    let offset = page_num * 10;
    event!(Level::INFO, "GET list of staff - at offset: {}",
        offset);

    let staff = conn
        .exec_map(
            "SELECT staff_id, first_name, last_name, email, username, password FROM staff ORDER BY last_update desc LIMIT 10 OFFSET :offset",
            params! {
                offset
            },
            |(staff_id, first_name, last_name, email, username, password)| {
                Staff { staff_id, first_name, last_name, email, username, password }
            },
        )?;

    Ok(staff)
}

fn get_staff(event: Request, conn: PooledConn) -> Result<Response<String>, Error> {
    let params = event.query_string_parameters();

    let page_num = match params.first("page") {
        Some(num) if num.starts_with("-") => 0,
        Some(num) => num.parse::<i32>().unwrap_or(0),
        _ => 0,
    };

    let showform = match params.first("new") {
        Some("t") => true,
        _ => false,
    };

    let staff = match params.first("staff_id") {
        Some(staff_id) => get_single_staff(conn, staff_id.to_string()),
        _ => get_list_staff(conn, page_num),
    }?;

    let pagination = Pagination {
        page: page_num,
        next: page_num + 1,
        prev: page_num - 1
    };

    // Return something that implements IntoResponse.
    // It will be serialized to the right response event automatically by the runtime
    // let body = serde_json::to_string(&staff).unwrap();

    let mut data = HashMap::new();
    data.insert("staff", to_json(&staff));
    data.insert("pagination", to_json(&pagination));
    data.insert("showform", to_json(&showform));

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("staff", include_str!("../templates/staff.hbs"))
        .unwrap();

    let body = handlebars.render("staff", &data).unwrap();
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(body)
        .map_err(Box::new)?;

    Ok(resp)
}

async fn router(
    method: &str,
    path: &str,
    event: Request,
    pool: PooledConn,
) -> Result<impl IntoResponse, Error> {
    let method_path = (method, path);
    match method_path {
        ("GET", "/staff") => get_staff(event, pool),
        ("POST", "/staff") => post_staff(event, pool),

        _ => panic!("Failed to match method and path"),
    }
}

/// This is the main body for the function.
/// Write your code inside it.
/// There are some code example in the following URLs:
/// <https://github.com/awslabs/aws-lambda-rust-runtime/tree/main/lambda-http/examples>
async fn function_handler(event: Request) -> Result<impl IntoResponse, Error> {
    let path = event.raw_http_path();

    let ctx = event.request_context();
    let method = match ctx {
        RequestContext::ApiGatewayV2(context) => context.http.method.to_string(),
        _ => "UNKNOWN".to_string(),
    };

    event!(Level::INFO, "Received {} request on {}", method, path);

    let url: String = env::var("MYSQL_URL").unwrap();
    let pool = Pool::new(Opts::from_url(&url)?)?;

    match pool.try_get_conn(1000) {
        Ok(conn) => router(&method, &path, event, conn).await,
        _ => panic!("Failed to connect to backend"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    run(service_fn(function_handler)).await
}
