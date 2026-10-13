use super::{files, identity, oidc, organizations};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

#[derive(OpenApi)]
#[openapi(paths(
    identity::signup, identity::request_verification, identity::verify_email, identity::login, identity::complete_mfa,
    identity::request_reset, identity::reset_password, identity::get_account, identity::deactivate_account, identity::change_password,
    identity::list_sessions, identity::revoke_session, identity::revoke_all_sessions, identity::rotate_session,
    identity::begin_totp, identity::confirm_totp, identity::disable_totp,
    organizations::create_organization, organizations::get_organization, organizations::list_organizations, organizations::rename_organization,
    oidc::begin_login, oidc::begin_link, oidc::callback, oidc::list_identities, oidc::unlink_identity,
    files::reserve_file, files::list_files, files::get_file, files::upload_original, files::delete_file,
    files::search_files, files::download_original, files::head_original
), components(schemas(super::error::ErrorOutput)), modifiers(&SecuritySchemes))]
struct ApiDocumentation;

struct SecuritySchemes;
impl Modify for SecuritySchemes {
    fn modify(&self, document: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = document.components.as_mut() {
            components.add_security_scheme("cookieAuth", SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description("__Host-session", "Development uses session; production uses __Host-session. Unsafe cookie requests require an allowed Origin."))));
            components.add_security_scheme(
                "bearerAuth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("Opaque session token")
                        .build(),
                ),
            );
        }
    }
}

pub fn document() -> utoipa::openapi::OpenApi {
    use utoipa::openapi::{Content, Ref, response::ResponseBuilder};
    let mut document = ApiDocumentation::openapi();
    for path in document.paths.paths.values_mut() {
        for operation in [
            &mut path.get,
            &mut path.post,
            &mut path.put,
            &mut path.delete,
            &mut path.patch,
            &mut path.head,
        ]
        .into_iter()
        .flatten()
        {
            for (status, description) in [
                ("400", "Invalid input"),
                ("401", "Authentication required"),
                ("403", "Access denied"),
                ("404", "Resource not found"),
                ("408", "Request deadline elapsed"),
                ("409", "Conflicting state"),
                ("413", "Body too large"),
                ("429", "Too many attempts"),
                ("500", "Internal error"),
                ("503", "Service unavailable"),
            ] {
                operation
                    .responses
                    .responses
                    .entry(status.into())
                    .or_insert_with(|| {
                        ResponseBuilder::new()
                            .description(description)
                            .content(
                                "application/json",
                                Content::new(Some(Ref::from_schema_name("ErrorOutput"))),
                            )
                            .build()
                            .into()
                    });
            }
        }
    }
    document
}
