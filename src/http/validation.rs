use validator::ValidationError;

pub fn email(value: &str) -> Result<(), ValidationError> {
    application::identity::validation::normalize_email(value)
        .map(|_| ())
        .map_err(|_| ValidationError::new("email"))
}

pub fn new_password(value: &str) -> Result<(), ValidationError> {
    application::identity::validation::validate_new_password(value)
        .map_err(|_| ValidationError::new("password_length"))
}

pub fn organization_name(value: &str) -> Result<(), ValidationError> {
    application::organizations::validation::normalize_name(value)
        .map(|_| ())
        .map_err(|_| ValidationError::new("organization_name"))
}

pub fn filename(value: &str) -> Result<(), ValidationError> {
    application::files::validation::validate_filename(value)
        .map_err(|_| ValidationError::new("filename"))
}

pub fn media_type(value: &str) -> Result<(), ValidationError> {
    application::files::validation::validate_media_type(value)
        .map_err(|_| ValidationError::new("media_type"))
}
