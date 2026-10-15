use super::{
    delete_file, download_original, get_file, head_original, list_files, reserve_file,
    search_files, upload_original,
};
use crate::http::HttpState;

pub(in crate::http::v1) fn routes() -> axum::Router<HttpState> {
    axum::Router::new()
        .route(
            "/organizations/{organization_id}/files",
            axum::routing::get(list_files::list_files).post(reserve_file::reserve_file),
        )
        .route(
            "/organizations/{organization_id}/files/search",
            axum::routing::get(search_files::search_files),
        )
        .route(
            "/organizations/{organization_id}/files/{file_id}",
            axum::routing::get(get_file::get_file).delete(delete_file::delete_file),
        )
        .route(
            "/organizations/{organization_id}/files/{file_id}/content",
            axum::routing::get(download_original::download_original)
                .head(head_original::head_original)
                .put(upload_original::upload_original),
        )
}
