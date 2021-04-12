//! This file contains the actix web server wrapper around the functions

mod database;
mod merkle;
mod hash;
mod hash_history;
mod merkle_storage;
mod build_merkle;
mod datasource;

use actix_web::{HttpServer, middleware, web};
use actix_web::web::Json;
use actix_web::{get, post};
use crate::database::MerkleSummary;
use actix_web::error::ErrorInternalServerError;
use crate::merkle::MerkleProof;
use crate::datasource::DataSource;
use crate::hash::HashValue;
use async_std::sync::Mutex;
use crate::hash_history::HashInfo;

#[macro_use] extern crate lazy_static;

#[get("/get_pending")]
async fn get_pending() -> actix_web::Result<Json<Vec<String>>> {
    Ok(Json(database::get_pending()))
}

#[derive(serde::Deserialize)]
struct AddToBoard {
    hash : String,
}

#[post("/add_to_board")]
async fn add_to_board(command : web::Json<AddToBoard>) -> Json<String> {
    database::add_item_to_merkle(&command.hash);
    Json("OK".to_string())
}

#[post("/initiate_merkle_now")]
async fn initiate_merkle_now() -> actix_web::Result<Json<[u8;32]>> {
    match database::initiate_merkle() {
        Ok(res) => Ok(Json(res)),
        Err(e) => {
            println!("{:?}",e);
            Err(ErrorInternalServerError("Could Not create"))
        }
    }

}

#[get("/get_merkle_trees")]
async fn get_merkle_trees() -> Json<Vec<MerkleSummary>> {
    Json(database::get_merkle_tree_summaries())
}

/// At the moment the proof request only applies to the last Merkle tree generated,
/// and requests by the leaf index, which is not very useful. So this will change significantly.
#[derive(serde::Deserialize)]
struct ProofRequest {
    index : usize,
}

// test with localhost:8090/get_merkle_proof?index=0
#[get("/get_merkle_proof")]
async fn get_merkle_proof(command:web::Query<ProofRequest>) -> actix_web::Result<Json<MerkleProof>> {
    println!("Trying to get merkle proof index {}",command.index);
    match database::get_proof(command.index) {
        Ok(res) => {
            println!("{:?}",res);
            Ok(Json(res))
        },
        Err(e) => {
            println!("{:?}",e);
            Err(ErrorInternalServerError("Could Not create"))
        }
    }
}

// new structures

#[derive(serde::Deserialize)]
struct Publish {
    data : String,
}

#[post("/submit_entry")]
async fn submit_entry(command : web::Json<Publish>,datasource: web::Data<Mutex<DataSource>>) -> Json<Result<HashValue,String>> {
    Json(datasource.lock().await.submit_leaf(&command.data).map_err(|e|e.to_string()))
}

#[get("/get_pending_hash_values")]
async fn get_pending_hash_values(datasource: web::Data<Mutex<DataSource>>) -> Json<Vec<HashValue>> {
    Json(datasource.lock().await.get_pending_hash_values())
}

#[get("/get_current_published_head")]
async fn get_current_published_head(datasource: web::Data<Mutex<DataSource>>) -> Json<Option<HashValue>> {
    Json(datasource.lock().await.get_current_published_head())
}


#[post("/request_new_published_head")]
async fn request_new_published_head(datasource: web::Data<Mutex<DataSource>>) -> Json<Result<HashValue,String>> {
    Json(datasource.lock().await.request_new_published_head().map_err(|e|e.to_string()))
}

#[derive(serde::Deserialize)]
struct QueryHash {
    hash : HashValue,
}

#[get("/lookup_hash")]
async fn lookup_hash(query:web::Query<QueryHash>,datasource: web::Data<Mutex<DataSource>>) -> Json<Option<HashInfo>> {
    Json(datasource.lock().await.lookup_hash(query.hash))
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    // make a dummy Merkle tree for testing purposes.
    database::add_item_to_merkle("Jane");
    database::add_item_to_merkle("Elizabeth");
    database::add_item_to_merkle("Mary");
    database::add_item_to_merkle("Catherine");
    database::add_item_to_merkle("Lydia");
    let _ = database::initiate_merkle();

    let datasource = web::Data::new(Mutex::new(DataSource::from_flatfiles()?));
    HttpServer::new(move|| {
        actix_web::App::new()
            .app_data(datasource.clone())
            .wrap(middleware::Compress::default())
            .service(submit_entry)
            .service(get_pending_hash_values)
            .service(get_current_published_head)
            .service(request_new_published_head)
            .service(lookup_hash)

            .service(get_pending)
            .service(add_to_board)
            .service(initiate_merkle_now)
            .service(get_merkle_trees)
            .service(get_merkle_proof)
            .service(actix_files::Files::new("/", "WebResources/")
                .use_last_modified(true).use_etag(true).index_file("index.html"))
    })
        .bind("0.0.0.0:8090")?
        .run()
        .await?;
    Ok(())
}