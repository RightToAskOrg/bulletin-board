//! This file contains the actix web server wrapper around the functions

mod database;
mod merkle;
mod hash;

use actix_web::{HttpServer, middleware, web};
use actix_web::web::Json;
use actix_web::{get, post};
use crate::database::MerkleSummary;
use actix_web::error::ErrorInternalServerError;
use crate::merkle::MerkleProof;

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

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    // make a dummy Merkle tree for testing purposes.
    database::add_item_to_merkle("Jane");
    database::add_item_to_merkle("Elizabeth");
    database::add_item_to_merkle("Mary");
    database::add_item_to_merkle("Catherine");
    database::add_item_to_merkle("Lydia");
    let _ = database::initiate_merkle();
    HttpServer::new(move|| {
        actix_web::App::new()
            .wrap(middleware::Compress::default())
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
        .await
}