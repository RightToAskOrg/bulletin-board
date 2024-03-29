//! This file contains the actix web server wrapper around the functions



use actix_web::{HttpServer, middleware, web};
use actix_web::web::Json;
use actix_web::{get, post};
use merkle_tree_bulletin_board::BulletinBoard;
use merkle_tree_bulletin_board::hash::HashValue;
use async_std::sync::Mutex;
use merkle_tree_bulletin_board::hash_history::{HashInfo, FullProof};
use std::path::PathBuf;
use merkle_tree_bulletin_board::backend_flatfile::BackendFlatfile;
use merkle_tree_bulletin_board::backend_journal::{BackendJournal, StartupVerification};

type OurBulletinBoard = BulletinBoard<BackendJournal<BackendFlatfile>>; // the actual type of the bulletin board.

#[derive(serde::Deserialize)]
struct Publish {
    data : String,
}

#[derive(serde::Deserialize)]
struct Censor {
    leaf_to_censor : HashValue,
}

#[post("/submit_leaf")]
async fn submit_leaf(command : web::Json<Publish>, datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<HashValue,String>> {
    Json(datasource.lock().await.submit_leaf(&command.data).map_err(|e|e.to_string()))
}

#[post("/censor_leaf")]
async fn censor_leaf(command : web::Json<Censor>, datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<(),String>> {
    Json(datasource.lock().await.censor_leaf(command.leaf_to_censor).map_err(|e|e.to_string()))
}


#[get("/get_parentless_unpublished_hash_values")]
async fn get_parentless_unpublished_hash_values(datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<Vec<HashValue>,String>> {
    Json(datasource.lock().await.get_parentless_unpublished_hash_values().map_err(|e|e.to_string()))
}

#[get("/get_most_recent_published_root")]
async fn get_most_recent_published_root(datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<Option<HashValue>,String>> {
    Json(datasource.lock().await.get_most_recent_published_root().map_err(|e|e.to_string()))
}


#[post("/order_new_published_root")]
async fn order_new_published_root(datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<HashValue,String>> {
    Json(datasource.lock().await.order_new_published_root().map_err(|e|e.to_string()))
}

#[derive(serde::Deserialize)]
struct QueryHash {
    hash : HashValue,
}

#[get("/get_hash_info")]
async fn get_hash_info(query:web::Query<QueryHash>, datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<HashInfo,String>> {
    Json(datasource.lock().await.get_hash_info(query.hash).map_err(|e|e.to_string()))
}

#[get("/get_proof_chain")]
async fn get_proof_chain(query:web::Query<QueryHash>, datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<FullProof,String>> {
    Json(datasource.lock().await.get_proof_chain(query.hash).map_err(|e|e.to_string()))
}

#[get("/get_all_published_roots")]
async fn get_all_published_roots(datasource: web::Data<Mutex<OurBulletinBoard>>) -> Json<Result<Vec<HashValue>,String>> {
    Json(datasource.lock().await.get_all_published_roots().map_err(|e|e.to_string()))
}

/// find the path containing web resources, static web files that will be served.
/// This is usually in the directory `WebResources` but the program may be run from
/// other directories. To be as robust as possible it will try likely possibilities.
fn find_web_resources() -> PathBuf {
    let rel_here = std::path::Path::new(".").canonicalize().expect("Could not resolve path .");
    for p in rel_here.ancestors() {
        let pp = p.join("WebResources");
        if pp.is_dir() {return pp;}
        let pp = p.join("bulletin-board-demo/WebResources");
        if pp.is_dir() {return pp;}
    }
    panic!("Could not find WebResources. Please run in a directory containing it.")
}


#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let backend_flatfile = BackendFlatfile::new("database.csv")?;
    let backend_journal = BackendJournal::new(backend_flatfile,"journal",StartupVerification::SanityCheckAndRepairPending)?;
    let datasource = web::Data::new(Mutex::new(BulletinBoard::new(backend_journal)?));
    println!("Running demo webserver on http://localhost:8090");
    HttpServer::new(move|| {
        actix_web::App::new()
            .app_data(datasource.clone())
            .wrap(middleware::Compress::default())
            .service(submit_leaf)
            .service(censor_leaf)
            .service(get_parentless_unpublished_hash_values)
            .service(get_most_recent_published_root)
            .service(order_new_published_root)
            .service(get_hash_info)
            .service(get_proof_chain)
            .service(get_all_published_roots)
            .service(actix_files::Files::new("/journal/", "journal").use_last_modified(true).use_etag(true).show_files_listing())
            .service(actix_files::Files::new("/", find_web_resources()).use_last_modified(true).use_etag(true).index_file("index.html"))
    })
        .bind("0.0.0.0:8090")?
        .run()
        .await?;
    Ok(())
}