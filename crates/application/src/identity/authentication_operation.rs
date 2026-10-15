#[derive(Clone, Copy)]
pub(super) enum AuthenticationOperation {
    Signup,
    Login,
    AuthenticationMail,
    VerifyEmail,
    ResetPassword,
    TotpSetup,
    Mfa,
    OidcStart,
    ChangePassword,
}
impl AuthenticationOperation {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Signup => "signup",
            Self::Login => "login",
            Self::AuthenticationMail => "authentication_mail",
            Self::VerifyEmail => "verify_email",
            Self::ResetPassword => "reset_password",
            Self::TotpSetup => "totp_setup",
            Self::Mfa => "mfa",
            Self::OidcStart => "oidc_start",
            Self::ChangePassword => "change_password",
        }
    }
}
