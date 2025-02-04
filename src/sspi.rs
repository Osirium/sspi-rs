/// The builders are required to compose and execute some of the `Sspi` methods.
pub mod builders;
pub mod internal;
pub mod kerberos;
#[cfg(windows)]
pub mod winapi;

mod ntlm;

use std::{error, fmt, io, result, str, string};

use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use picky_asn1::restricted_string::CharSetError;
use picky_asn1_der::Asn1DerError;
use picky_krb::gss_api::GssApiMessageError;
use picky_krb::messages::KrbError;

use self::builders::{
    AcceptSecurityContext, AcquireCredentialsHandle, EmptyAcceptSecurityContext, EmptyAcquireCredentialsHandle,
    EmptyInitializeSecurityContext, FilledAcceptSecurityContext, FilledAcquireCredentialsHandle,
    FilledInitializeSecurityContext, InitializeSecurityContext,
};
pub use self::builders::{
    AcceptSecurityContextResult, AcquireCredentialsHandleResult, InitializeSecurityContextResult,
};
use self::internal::SspiImpl;
pub use self::ntlm::{AuthIdentity, AuthIdentityBuffers, Ntlm};

/// Representation of SSPI-related result operation. Makes it easier to return a `Result` with SSPI-related `Error`.
pub type Result<T> = result::Result<T, Error>;
pub type Luid = u64;

const PACKAGE_ID_NONE: u16 = 0xFFFF;

/// Retrieves information about a specified security package. This information includes credentials and contexts.
///
/// # Returns
///
/// * `PackageInfo` containing the information about the security principal upon success
/// * `Error` on error
///
/// # Example
///
/// ```
/// let package_info = sspi::query_security_package_info(sspi::SecurityPackageType::Ntlm)
///     .unwrap();
/// println!("Package info:");
/// println!("Name: {:?}", package_info.name);
/// println!("Comment: {}", package_info.comment);
/// ```
///
/// # MSDN
///
/// * [QuerySecurityPackageInfoW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-querysecuritypackageinfow)
pub fn query_security_package_info(package_type: SecurityPackageType) -> Result<PackageInfo> {
    match package_type {
        SecurityPackageType::Ntlm => Ok(ntlm::PACKAGE_INFO.clone()),
        SecurityPackageType::Kerberos => Ok(kerberos::PACKAGE_INFO.clone()),
        SecurityPackageType::Other(s) => Err(Error::new(
            ErrorKind::Unknown,
            format!("Queried info about unknown package: {:?}", s),
        )),
    }
}

/// Returns an array of `PackageInfo` structures that provide information about the security packages available to the client.
///
/// # Returns
///
/// * `Vec` of `PackageInfo` structures upon success
/// * `Error` on error
///
/// # Example
///
/// ```
/// let packages = sspi::enumerate_security_packages().unwrap();
///
/// println!("Available packages:");
/// for ssp in packages {
///     println!("{:?}", ssp.name);
/// }
/// ```
///
/// # MSDN
///
/// * [EnumerateSecurityPackagesW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-enumeratesecuritypackagesw)
pub fn enumerate_security_packages() -> Result<Vec<PackageInfo>> {
    Ok(vec![
        kerberos::PACKAGE_INFO.clone(),
        kerberos::NEGO_PACKAGE_INFO.clone(),
    ])
}

/// This trait provides interface for all available SSPI functions. The `acquire_credentials_handle`,
/// `initialize_security_context`, and `accept_security_context` methods return Builders that make it
/// easier to assemble the list of arguments for the function and then execute it.
///
/// # MSDN
///
/// * [SSPI.h](https://docs.microsoft.com/en-us/windows/win32/api/sspi/)
pub trait Sspi
where
    Self: Sized + SspiImpl,
{
    /// Acquires a handle to preexisting credentials of a security principal. The preexisting credentials are
    /// available only for `sspi::winapi` module. This handle is required by the `initialize_security_context`
    /// and `accept_security_context` functions. These can be either preexisting credentials, which are
    /// established through a system logon, or the caller can provide alternative credentials. Alternative
    /// credentials are always required to specify when using platform independent SSPs.
    ///
    /// # Returns
    ///
    /// * `AcquireCredentialsHandle` builder
    ///
    /// # Requirements for execution
    ///
    /// These methods are required to be called before calling the `execute` method of the `AcquireCredentialsHandle` builder:
    /// * [`with_credential_use`](builders/struct.AcquireCredentialsHandle.html#method.with_credential_use)
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// #
    /// # let mut ntlm = sspi::Ntlm::new();
    /// #
    /// let identity = sspi::AuthIdentity {
    ///     username: "user".to_string(),
    ///     password: "password".to_string(),
    ///     domain: None,
    /// };
    ///
    /// # #[allow(unused_variables)]
    /// let result = ntlm
    ///     .acquire_credentials_handle()
    ///     .with_credential_use(sspi::CredentialUse::Outbound)
    ///     .with_auth_data(&identity)
    ///     .execute()
    ///     .unwrap();
    /// ```
    ///
    /// # MSDN
    ///
    /// * [AcquireCredentialshandleW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acquirecredentialshandlew)
    fn acquire_credentials_handle(
        &mut self,
    ) -> EmptyAcquireCredentialsHandle<'_, Self, Self::CredentialsHandle, Self::AuthenticationData> {
        AcquireCredentialsHandle::new(self)
    }

    /// Initiates the client side, outbound security context from a credential handle.
    /// The function is used to build a security context between the client application and a remote peer. The function returns a token
    /// that the client must pass to the remote peer, which the peer in turn submits to the local security implementation through the
    /// `accept_security_context` call.
    ///
    /// # Returns
    ///
    /// * `InitializeSecurityContext` builder
    ///
    /// # Requirements for execution
    ///
    /// These methods are required to be called before calling the `execute` method
    /// * [`with_credentials_handle`](builders/struct.InitializeSecurityContext.html#method.with_credentials_handle)
    /// * [`with_context_requirements`](builders/struct.InitializeSecurityContext.html#method.with_context_requirements)
    /// * [`with_target_data_representation`](builders/struct.InitializeSecurityContext.html#method.with_target_data_representation)
    /// * [`with_output`](builders/struct.InitializeSecurityContext.html#method.with_output)
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// #
    /// # let mut ntlm = sspi::Ntlm::new();
    /// #
    /// # let identity = sspi::AuthIdentity {
    /// #     username: whoami::username(),
    /// #     password: String::from("password"),
    /// #     domain: Some(whoami::hostname()),
    /// # };
    /// #
    /// # let mut acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Outbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut credentials_handle = acq_cred_result.credentials_handle;
    /// #
    /// let mut output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    ///
    /// # #[allow(unused_variables)]
    /// let result = ntlm
    ///     .initialize_security_context()
    ///     .with_credentials_handle(&mut credentials_handle)
    ///     .with_context_requirements(
    ///         sspi::ClientRequestFlags::CONFIDENTIALITY | sspi::ClientRequestFlags::ALLOCATE_MEMORY,
    ///     )
    ///     .with_target_data_representation(sspi::DataRepresentation::Native)
    ///     .with_output(&mut output_buffer)
    ///     .execute()
    ///     .unwrap();
    /// ```
    ///
    /// # MSDN
    ///
    /// * [InitializeSecurityContextW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-initializesecuritycontextw)
    fn initialize_security_context(&mut self) -> EmptyInitializeSecurityContext<'_, Self, Self::CredentialsHandle> {
        InitializeSecurityContext::new(self)
    }

    /// Lets the server component of a transport application establish a security context between the server and a remote client.
    /// The remote client calls the `initialize_security_context` function to start the process of establishing a security context.
    /// The server can require one or more reply tokens from the remote client to complete establishing the security context.
    ///
    /// # Returns
    ///
    /// * `AcceptSecurityContext` builder
    ///
    /// # Requirements for execution
    ///
    /// These methods are required to be called before calling the `execute` method of the `AcceptSecurityContext` builder:
    /// * [`with_credentials_handle`](builders/struct.AcceptSecurityContext.html#method.with_credentials_handle)
    /// * [`with_context_requirements`](builders/struct.AcceptSecurityContext.html#method.with_context_requirements)
    /// * [`with_target_data_representation`](builders/struct.AcceptSecurityContext.html#method.with_target_data_representation)
    /// * [`with_output`](builders/struct.AcceptSecurityContext.html#method.with_output)
    ///
    /// # Example
    ///
    /// ```
    /// #  use sspi::Sspi;
    /// #
    /// # let mut client_ntlm = sspi::Ntlm::new();
    /// #
    /// # let identity = sspi::AuthIdentity {
    /// #     username: "user".to_string(),
    /// #     password: "password".to_string(),
    /// #     domain: None,
    /// # };
    /// #
    /// # let mut client_acq_cred_result = client_ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Outbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut client_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// #
    /// # let _result = client_ntlm
    /// #     .initialize_security_context()
    /// #     .with_credentials_handle(&mut client_acq_cred_result.credentials_handle)
    /// #     .with_context_requirements(
    /// #         sspi::ClientRequestFlags::CONFIDENTIALITY | sspi::ClientRequestFlags::ALLOCATE_MEMORY,
    /// #     )
    /// #     .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #     .with_target_name("user")
    /// #     .with_output(&mut client_output_buffer)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// let mut ntlm = sspi::Ntlm::new();
    /// let mut output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// #
    /// # let mut server_acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Inbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut credentials_handle = server_acq_cred_result.credentials_handle;
    ///
    /// # #[allow(unused_variables)]
    /// let result = ntlm
    ///     .accept_security_context()
    ///     .with_credentials_handle(&mut credentials_handle)
    ///     .with_context_requirements(sspi::ServerRequestFlags::ALLOCATE_MEMORY)
    ///     .with_target_data_representation(sspi::DataRepresentation::Native)
    ///     .with_input(&mut client_output_buffer)
    ///     .with_output(&mut output_buffer)
    ///     .execute()
    ///     .unwrap();
    /// ```
    ///
    /// # MSDN
    ///
    /// * [AcceptSecurityContext function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acceptsecuritycontext)
    fn accept_security_context(&mut self) -> EmptyAcceptSecurityContext<'_, Self, Self::CredentialsHandle> {
        AcceptSecurityContext::new(self)
    }

    /// Completes an authentication token. This function is used by protocols, such as DCE,
    /// that need to revise the security information after the transport application has updated some message parameters.
    ///
    /// # Parameters
    ///
    /// * `token`: `SecurityBuffer` that contains the buffer descriptor for the entire message
    ///
    /// # Returns
    ///
    /// * `SspiOk` on success
    /// * `Error` on error
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// #
    /// # let mut client_ntlm = sspi::Ntlm::new();
    /// # let mut ntlm = sspi::Ntlm::new();
    /// #
    /// # let mut client_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// # let mut output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// #
    /// # let identity = sspi::AuthIdentity {
    /// #     username: "user".to_string(),
    /// #     password: "password".to_string(),
    /// #     domain: None,
    /// # };
    /// #
    /// # let mut client_acq_cred_result = client_ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Outbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut server_acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Inbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # loop {
    /// #     client_output_buffer[0].buffer.clear();
    /// #
    /// #     let _client_result = client_ntlm
    /// #         .initialize_security_context()
    /// #         .with_credentials_handle(&mut client_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(
    /// #             sspi::ClientRequestFlags::CONFIDENTIALITY | sspi::ClientRequestFlags::ALLOCATE_MEMORY,
    /// #         )
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_target_name("user")
    /// #         .with_input(&mut output_buffer)
    /// #         .with_output(&mut client_output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     let server_result = ntlm
    /// #         .accept_security_context()
    /// #         .with_credentials_handle(&mut server_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(sspi::ServerRequestFlags::ALLOCATE_MEMORY)
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_input(&mut client_output_buffer)
    /// #         .with_output(&mut output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     if server_result.status == sspi::SecurityStatus::CompleteAndContinue
    /// #         || server_result.status == sspi::SecurityStatus::CompleteNeeded
    /// #     {
    /// #         break;
    /// #     }
    /// # }
    /// #
    /// # #[allow(unused_variables)]
    /// let result = ntlm
    ///     .complete_auth_token(&mut output_buffer)
    ///     .unwrap();
    /// ```
    ///
    /// # MSDN
    ///
    /// * [CompleteAuthToken function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-completeauthtoken)
    fn complete_auth_token(&mut self, token: &mut [SecurityBuffer]) -> Result<SecurityStatus>;

    /// Encrypts a message to provide privacy. The function allows the application to choose among cryptographic algorithms supported by the chosen mechanism.
    /// Some packages do not have messages to be encrypted or decrypted but rather provide an integrity hash that can be checked.
    ///
    /// # Parameters
    ///
    /// * `flags`: package-specific flags that indicate the quality of protection. A security package can use this parameter to enable the selection of cryptographic algorithms
    /// * `message`: on input, the structure accepts one or more `SecurityBuffer` structures that can be of type `SecurityBufferType::Data`.
    /// That buffer contains the message to be encrypted. The message is encrypted in place, overwriting the original contents of the structure.
    /// * `sequence_number`: the sequence number that the transport application assigned to the message. If the transport application does not maintain sequence numbers, this parameter must be zero
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut client_ntlm = sspi::Ntlm::new();
    /// # let mut ntlm = sspi::Ntlm::new();
    /// #
    /// # let mut client_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// # let mut server_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// #
    /// # let identity = sspi::AuthIdentity {
    /// #     username: "user".to_string(),
    /// #     password: "password".to_string(),
    /// #     domain: None,
    /// # };
    /// #
    /// # let mut client_acq_cred_result = client_ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Outbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut server_acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Inbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # loop {
    /// #     client_output_buffer[0].buffer.clear();
    /// #
    /// #     let _client_result = client_ntlm
    /// #         .initialize_security_context()
    /// #         .with_credentials_handle(&mut client_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(
    /// #             sspi::ClientRequestFlags::CONFIDENTIALITY | sspi::ClientRequestFlags::ALLOCATE_MEMORY,
    /// #         )
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_target_name("user")
    /// #         .with_input(&mut server_output_buffer)
    /// #         .with_output(&mut client_output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     let server_result = ntlm
    /// #         .accept_security_context()
    /// #         .with_credentials_handle(&mut server_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(sspi::ServerRequestFlags::ALLOCATE_MEMORY)
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_input(&mut client_output_buffer)
    /// #         .with_output(&mut server_output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     if server_result.status == sspi::SecurityStatus::CompleteAndContinue
    /// #         || server_result.status == sspi::SecurityStatus::CompleteNeeded
    /// #     {
    /// #         break;
    /// #     }
    /// # }
    /// #
    /// # let _result = ntlm
    /// #     .complete_auth_token(&mut server_output_buffer)
    /// #     .unwrap();
    /// #
    /// let mut msg_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token),
    ///     sspi::SecurityBuffer::new(Vec::from("This is a message".as_bytes()), sspi::SecurityBufferType::Data)];
    ///
    /// println!("Unencrypted: {:?}", msg_buffer[1].buffer);
    ///
    /// # #[allow(unused_variables)]
    /// let result = ntlm
    ///     .encrypt_message(sspi::EncryptionFlags::empty(), &mut msg_buffer, 0).unwrap();
    ///
    /// println!("Encrypted: {:?}", msg_buffer[1].buffer);
    /// ```
    ///
    /// # Returns
    ///
    /// * `SspiOk` on success
    /// * `Error` on error
    ///
    /// # MSDN
    ///
    /// * [EncryptMessage function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-encryptmessage)
    fn encrypt_message(
        &mut self,
        flags: EncryptionFlags,
        message: &mut [SecurityBuffer],
        sequence_number: u32,
    ) -> Result<SecurityStatus>;

    /// Decrypts a message. Some packages do not encrypt and decrypt messages but rather perform and check an integrity hash.
    ///
    /// # Parameters
    ///
    /// * `message`: on input, the structure references one or more `SecurityBuffer` structures.
    /// At least one of these must be of type `SecurityBufferType::Data`.
    /// That buffer contains the encrypted message. The encrypted message is decrypted in place, overwriting the original contents of its buffer
    /// * `sequence_number`: the sequence number that the transport application assigned to the message. If the transport application does not maintain sequence numbers, this parameter must be zero
    ///
    /// # Returns
    ///
    /// * `DecryptionFlags` upon success
    /// * `Error` on error
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut ntlm = sspi::Ntlm::new();
    /// # let mut server_ntlm = sspi::Ntlm::new();
    /// #
    /// # let mut client_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// # let mut server_output_buffer = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token)];
    /// #
    /// # let identity = sspi::AuthIdentity {
    /// #     username: "user".to_string(),
    /// #     password: "password".to_string(),
    /// #     domain: None,
    /// # };
    /// #
    /// # let mut client_acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Outbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # let mut server_acq_cred_result = server_ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Inbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute()
    /// #     .unwrap();
    /// #
    /// # loop {
    /// #     client_output_buffer[0].buffer.clear();
    /// #
    /// #     let _client_result = ntlm
    /// #         .initialize_security_context()
    /// #         .with_credentials_handle(&mut client_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(
    /// #             sspi::ClientRequestFlags::CONFIDENTIALITY | sspi::ClientRequestFlags::ALLOCATE_MEMORY,
    /// #         )
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_target_name("user")
    /// #         .with_input(&mut server_output_buffer)
    /// #         .with_output(&mut client_output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     let server_result = server_ntlm
    /// #         .accept_security_context()
    /// #         .with_credentials_handle(&mut server_acq_cred_result.credentials_handle)
    /// #         .with_context_requirements(sspi::ServerRequestFlags::ALLOCATE_MEMORY)
    /// #         .with_target_data_representation(sspi::DataRepresentation::Native)
    /// #         .with_input(&mut client_output_buffer)
    /// #         .with_output(&mut server_output_buffer)
    /// #         .execute()
    /// #         .unwrap();
    /// #
    /// #     if server_result.status == sspi::SecurityStatus::CompleteAndContinue
    /// #         || server_result.status == sspi::SecurityStatus::CompleteNeeded
    /// #     {
    /// #         break;
    /// #     }
    /// # }
    /// #
    /// # let _result = server_ntlm
    /// #     .complete_auth_token(&mut server_output_buffer)
    /// #     .unwrap();
    /// #
    /// # let mut msg = vec![sspi::SecurityBuffer::new(Vec::new(), sspi::SecurityBufferType::Token),
    /// #     sspi::SecurityBuffer::new(Vec::from("This is a message".as_bytes()), sspi::SecurityBufferType::Data)];
    /// #
    /// # let _result = server_ntlm
    /// #     .encrypt_message(sspi::EncryptionFlags::empty(), &mut msg, 0).unwrap();
    /// #
    /// # let mut msg_buffer = vec![
    /// #     sspi::SecurityBuffer::new(msg[0].buffer.clone(), sspi::SecurityBufferType::Token),
    /// #     sspi::SecurityBuffer::new(msg[1].buffer.clone(), sspi::SecurityBufferType::Data),
    /// # ];
    /// #
    /// # #[allow(unused_variables)]
    /// let encryption_flags = ntlm
    ///     .decrypt_message(&mut msg_buffer, 0)
    ///     .unwrap();
    ///
    /// println!("Decrypted message: {:?}", msg_buffer[1].buffer);
    /// ```
    ///
    /// # MSDN
    ///
    /// * [DecryptMessage function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-decryptmessage)
    fn decrypt_message(&mut self, message: &mut [SecurityBuffer], sequence_number: u32) -> Result<DecryptionFlags>;

    /// Retrieves information about the bounds of sizes of authentication information of the current security principal.
    ///
    /// # Returns
    ///
    /// * `ContextSizes` upon success
    /// * `Error` on error
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut ntlm = sspi::Ntlm::new();
    /// let sizes = ntlm.query_context_sizes().unwrap();
    /// println!("Max token: {}", sizes.max_token);
    /// println!("Max signature: {}", sizes.max_signature);
    /// println!("Block: {}", sizes.block);
    /// println!("Security trailer: {}", sizes.security_trailer);
    /// ```
    ///
    /// # MSDN
    ///
    /// * [QueryCredentialsAttributesW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-querycredentialsattributesw)
    fn query_context_sizes(&mut self) -> Result<ContextSizes>;

    /// Retrieves the username of the credential associated to the context.
    ///
    /// # Returns
    ///
    /// * `ContextNames` upon success
    /// * `Error` on error
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut ntlm = sspi::Ntlm::new();
    /// # let identity = sspi::AuthIdentity {
    /// #     username: "user".to_string(),
    /// #     password: "password".to_string(),
    /// #     domain: None,
    /// # };
    /// #
    /// # let _acq_cred_result = ntlm
    /// #     .acquire_credentials_handle()
    /// #     .with_credential_use(sspi::CredentialUse::Inbound)
    /// #     .with_auth_data(&identity)
    /// #     .execute().unwrap();
    /// #
    /// let names = ntlm.query_context_names().unwrap();
    /// println!("Username: {:?}", names.username);
    /// println!("Domain: {:?}", names.domain);
    /// ```
    ///
    /// # MSDN
    ///
    /// * [QuerySecurityPackageInfoW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-querysecuritypackageinfow)
    fn query_context_names(&mut self) -> Result<ContextNames>;

    /// Retrieves information about the specified security package. This information includes the bounds of sizes of authentication information, credentials, and contexts.
    ///
    /// # Returns
    ///
    /// * `PackageInfo` containing the information about the package
    /// * `Error` on error
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut ntlm = sspi::Ntlm::new();
    /// let info = ntlm.query_context_package_info().unwrap();
    /// println!("Package name: {:?}", info.name);
    /// ```
    ///
    /// # MSDN
    ///
    /// * [QuerySecurityPackageInfoW function](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-querysecuritypackageinfow)
    fn query_context_package_info(&mut self) -> Result<PackageInfo>;

    /// Retrieves the trust information of the certificate.
    ///
    /// # Returns
    ///
    /// * `CertTrustStatus` on success
    ///
    /// # Example
    ///
    /// ```
    /// # use sspi::Sspi;
    /// # let mut ntlm = sspi::Ntlm::new();
    /// let cert_info = ntlm.query_context_package_info().unwrap();
    /// ```
    ///
    /// # MSDN
    ///
    /// * [QueryContextAttributes (CredSSP) function (`ulAttribute` parameter)](https://docs.microsoft.com/en-us/windows/win32/secauthn/querycontextattributes--credssp)
    fn query_context_cert_trust_status(&mut self) -> Result<CertTrustStatus>;
}

pub trait SspiEx
where
    Self: Sized + SspiImpl,
{
    fn custom_set_auth_identity(&mut self, identity: Self::AuthenticationData);
}

bitflags! {
    /// Indicate the quality of protection. Used in the `encrypt_message` method.
    ///
    /// # MSDN
    ///
    /// * [EncryptMessage function (`fQOP` parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-encryptmessage)
    pub struct EncryptionFlags: u32 {
        const WRAP_OOB_DATA = 0x4000_0000;
        const WRAP_NO_ENCRYPT = 0x8000_0001;
    }
}

bitflags! {
    /// Indicate the quality of protection. Returned by the `decrypt_message` method.
    ///
    /// # MSDN
    ///
    /// * [DecryptMessage function (`pfQOP` parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-decryptmessage)
    pub struct DecryptionFlags: u32 {
        const SIGN_ONLY = 0x8000_0000;
        const WRAP_NO_ENCRYPT = 0x8000_0001;
    }
}

bitflags! {
    /// Indicate requests for the context. Not all packages can support all requirements. Bit flags can be combined by using bitwise-OR operations.
    ///
    /// # MSDN
    ///
    /// * [Context Requirements](https://docs.microsoft.com/en-us/windows/win32/secauthn/context-requirements)
    /// * [InitializeSecurityContextW function (fContextReq parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-initializesecuritycontextw)
    pub struct ClientRequestFlags: u32 {
        /// The server can use the context to authenticate to other servers as the client.
        /// The `MUTUAL_AUTH` flag must be set for this flag to work. Valid for Kerberos. Ignore this flag for constrained delegation.
        const DELEGATE = 0x1;
        /// The mutual authentication policy of the service will be satisfied.
        const MUTUAL_AUTH = 0x2;
        /// Detect replayed messages that have been encoded by using the `encrypt_message` or `make_signature` (TBI) functions.
        const REPLAY_DETECT = 0x4;
        /// Detect messages received out of sequence.
        const SEQUENCE_DETECT = 0x8;
        /// Encrypt messages by using the `encrypt_message` function.
        const CONFIDENTIALITY = 0x10;
        /// A new session key must be negotiated. This value is supported only by the Kerberos security package.
        const USE_SESSION_KEY = 0x20;
        const PROMPT_FOR_CREDS = 0x40;
        /// Schannel must not attempt to supply credentials for the client automatically.
        const USE_SUPPLIED_CREDS = 0x80;
        /// The security package allocates output buffers for you.
        const ALLOCATE_MEMORY = 0x100;
        const USE_DCE_STYLE = 0x200;
        const DATAGRAM = 0x400;
        /// The security context will not handle formatting messages. This value is the default for the Kerberos, Negotiate, and NTLM security packages.
        const CONNECTION = 0x800;
        const CALL_LEVEL = 0x1000;
        const FRAGMENT_SUPPLIED = 0x2000;
        /// When errors occur, the remote party will be notified.
        const EXTENDED_ERROR = 0x4000;
        /// Support a stream-oriented connection.
        const STREAM = 0x8000;
        /// Sign messages and verify signatures by using the `encrypt_message` and `make_signature` (TBI) functions.
        const INTEGRITY = 0x0001_0000;
        const IDENTIFY = 0x0002_0000;
        const NULL_SESSION = 0x0004_0000;
        /// Schannel must not authenticate the server automatically.
        const MANUAL_CRED_VALIDATION = 0x0008_0000;
        const RESERVED1 = 0x0010_0000;
        const FRAGMENT_TO_FIT = 0x0020_0000;
        const FORWARD_CREDENTIALS = 0x0040_0000;
        /// If this flag is set, the `Integrity` flag is ignored. This value is supported only by the Negotiate and Kerberos security packages.
        const NO_INTEGRITY = 0x0080_0000;
        const USE_HTTP_STYLE = 0x100_0000;
        const UNVERIFIED_TARGET_NAME = 0x2000_0000;
        const CONFIDENTIALITY_ONLY = 0x4000_0000;
    }
}

bitflags! {
    /// Specify the attributes required by the server to establish the context. Bit flags can be combined by using bitwise-OR operations.
    ///
    /// # MSDN
    ///
    /// * [Context Requirements](https://docs.microsoft.com/en-us/windows/win32/secauthn/context-requirements)
    /// * [AcceptSecurityContext function function (fContextReq parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acceptsecuritycontext?redirectedfrom=MSDN)
    pub struct ServerRequestFlags: u32 {
        /// The server is allowed to impersonate the client. Ignore this flag for [constrained delegation](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly).
        const DELEGATE = 0x1;
        const MUTUAL_AUTH = 0x2;
        /// Detect replayed packets.
        const REPLAY_DETECT = 0x4;
        /// Detect messages received out of sequence.
        const SEQUENCE_DETECT = 0x8;
        const CONFIDENTIALITY = 0x10;
        const USE_SESSION_KEY = 0x20;
        const SESSION_TICKET = 0x40;
        /// Credential Security Support Provider (CredSSP) will allocate output buffers.
        const ALLOCATE_MEMORY = 0x100;
        const USE_DCE_STYLE = 0x200;
        const DATAGRAM = 0x400;
        /// The security context will not handle formatting messages.
        const CONNECTION = 0x800;
        const CALL_LEVEL = 0x1000;
        const FRAGMENT_SUPPLIED = 0x2000;
        /// When errors occur, the remote party will be notified.
        const EXTENDED_ERROR = 0x8000;
        /// Support a stream-oriented connection.
        const STREAM = 0x0001_0000;
        const INTEGRITY = 0x0002_0000;
        const LICENSING = 0x0004_0000;
        const IDENTIFY = 0x0008_0000;
        const ALLOW_NULL_SESSION = 0x0010_0000;
        const ALLOW_NON_USER_LOGONS = 0x0020_0000;
        const ALLOW_CONTEXT_REPLAY = 0x0040_0000;
        const FRAGMENT_TO_FIT = 0x80_0000;
        const NO_TOKEN = 0x100_0000;
        const PROXY_BINDINGS = 0x400_0000;
        const ALLOW_MISSING_BINDINGS = 0x1000_0000;
    }
}

bitflags! {
    /// Indicate the attributes of the established context.
    ///
    /// # MSDN
    ///
    /// * [Context Requirements](https://docs.microsoft.com/en-us/windows/win32/secauthn/context-requirements)
    /// * [InitializeSecurityContextW function (pfContextAttr parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-initializesecuritycontextw)
    pub struct ClientResponseFlags: u32 {
        /// The server can use the context to authenticate to other servers as the client.
        /// The `MUTUAL_AUTH` flag must be set for this flag to work. Valid for Kerberos. Ignore this flag for constrained delegation.
        const DELEGATE = 0x1;
        /// The mutual authentication policy of the service will be satisfied.
        const MUTUAL_AUTH = 0x2;
        /// Detect replayed messages that have been encoded by using the `encrypt_message` or `make_signature` (TBI) functions.
        const REPLAY_DETECT = 0x4;
        /// Detect messages received out of sequence.
        const SEQUENCE_DETECT = 0x8;
        /// Encrypt messages by using the `encrypt_message` function.
        const CONFIDENTIALITY = 0x10;
        /// A new session key must be negotiated. This value is supported only by the Kerberos security package.
        const USE_SESSION_KEY = 0x20;
        const USED_COLLECTED_CREDS = 0x40;
        /// Schannel must not attempt to supply credentials for the client automatically.
        const USED_SUPPLIED_CREDS = 0x80;
        /// The security package allocates output buffers for you.
        const ALLOCATED_MEMORY = 0x100;
        const USED_DCE_STYLE = 0x200;
        const DATAGRAM = 0x400;
        /// The security context will not handle formatting messages. This value is the default for the Kerberos, Negotiate, and NTLM security packages.
        const CONNECTION = 0x800;
        const INTERMEDIATE_RETURN = 0x1000;
        const CALL_LEVEL = 0x2000;
        /// When errors occur, the remote party will be notified.
        const EXTENDED_ERROR = 0x4000;
        /// Support a stream-oriented connection.
        const STREAM = 0x8000;
        /// Sign messages and verify signatures by using the `encrypt_message` and `make_signature` (TBI) functions.
        const INTEGRITY = 0x0001_0000;
        const IDENTIFY = 0x0002_0000;
        const NULL_SESSION = 0x0004_0000;
        /// Schannel must not authenticate the server automatically.
        const MANUAL_CRED_VALIDATION = 0x0008_0000;
        const RESERVED1 = 0x10_0000;
        const FRAGMENT_ONLY = 0x0020_0000;
        const FORWARD_CREDENTIALS = 0x0040_0000;
        const USED_HTTP_STYLE = 0x100_0000;
        const NO_ADDITIONAL_TOKEN = 0x200_0000;
        const REAUTHENTICATION = 0x800_0000;
        const CONFIDENTIALITY_ONLY = 0x4000_0000;
    }
}

bitflags! {
    /// Indicate the attributes of the established context.
    ///
    /// # MSDN
    ///
    /// * [Context Requirements](https://docs.microsoft.com/en-us/windows/win32/secauthn/context-requirements)
    /// * [AcceptSecurityContext function function (pfContextAttr parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acceptsecuritycontext?redirectedfrom=MSDN)
    pub struct ServerResponseFlags: u32 {
        /// The server is allowed to impersonate the client. Ignore this flag for [constrained delegation](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly).
        const DELEGATE = 0x1;
        const MUTUAL_AUTH = 0x2;
        /// Detect replayed packets.
        const REPLAY_DETECT = 0x4;
        /// Detect messages received out of sequence.
        const SEQUENCE_DETECT = 0x8;
        const CONFIDENTIALITY = 0x10;
        const USE_SESSION_KEY = 0x20;
        const SESSION_TICKET = 0x40;
        /// Credential Security Support Provider (CredSSP) will allocate output buffers.
        const ALLOCATED_MEMORY = 0x100;
        const USED_DCE_STYLE = 0x200;
        const DATAGRAM = 0x400;
        /// The security context will not handle formatting messages.
        const CONNECTION = 0x800;
        const CALL_LEVEL = 0x2000;
        const THIRD_LEG_FAILED = 0x4000;
        /// When errors occur, the remote party will be notified.
        const EXTENDED_ERROR = 0x8000;
        /// Support a stream-oriented connection.
        const STREAM = 0x0001_0000;
        const INTEGRITY = 0x0002_0000;
        const LICENSING = 0x0004_0000;
        const IDENTIFY = 0x0008_0000;
        const NULL_SESSION = 0x0010_0000;
        const ALLOW_NON_USER_LOGONS = 0x0020_0000;
        const ALLOW_CONTEXT_REPLAY = 0x0040_0000;
        const FRAGMENT_ONLY = 0x0080_0000;
        const NO_TOKEN = 0x100_0000;
        const NO_ADDITIONAL_TOKEN = 0x200_0000;
    }
}

/// The data representation, such as byte ordering, on the target.
///
/// # MSDN
///
/// * [AcceptSecurityContext function (TargetDataRep parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acceptsecuritycontext)
#[derive(Debug, Copy, Clone, Eq, PartialEq, FromPrimitive, ToPrimitive)]
pub enum DataRepresentation {
    Network = 0,
    Native = 0x10,
}

/// Describes a buffer allocated by a transport application to pass to a security package.
///
/// # MSDN
///
/// * [SecBuffer structure](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secbuffer)
#[derive(Debug, Clone)]
pub struct SecurityBuffer {
    pub buffer: Vec<u8>,
    pub buffer_type: SecurityBufferType,
}

impl SecurityBuffer {
    pub fn new(buffer: Vec<u8>, buffer_type: SecurityBufferType) -> Self {
        Self { buffer, buffer_type }
    }

    pub fn find_buffer(buffers: &[SecurityBuffer], buffer_type: SecurityBufferType) -> Result<&SecurityBuffer> {
        buffers.iter().find(|b| b.buffer_type == buffer_type).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidToken,
                format!("No buffer was provided with type {:?}", buffer_type),
            )
        })
    }

    pub fn find_buffer_mut(
        buffers: &mut [SecurityBuffer],
        buffer_type: SecurityBufferType,
    ) -> Result<&mut SecurityBuffer> {
        buffers
            .iter_mut()
            .find(|b| b.buffer_type == buffer_type)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidToken,
                    format!("No buffer was provided with type {:?}", buffer_type),
                )
            })
    }
}

/// Bit flags that indicate the type of buffer.
///
/// # MSDN
///
/// * [SecBuffer structure (BufferType parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secbuffer)
#[repr(u32)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, FromPrimitive, ToPrimitive)]
pub enum SecurityBufferType {
    Empty = 0,
    /// The buffer contains common data. The security package can read and write this data, for example, to encrypt some or all of it.
    Data = 1,
    /// The buffer contains the security token portion of the message. This is read-only for input parameters or read/write for output parameters.
    Token = 2,
    TransportToPackageParameters = 3,
    /// The security package uses this value to indicate the number of missing bytes in a particular message.
    Missing = 4,
    /// The security package uses this value to indicate the number of extra or unprocessed bytes in a message.
    Extra = 5,
    /// The buffer contains a protocol-specific trailer for a particular record. It is not usually of interest to callers.
    StreamTrailer = 6,
    /// The buffer contains a protocol-specific header for a particular record. It is not usually of interest to callers.
    StreamHeader = 7,
    NegotiationInfo = 8,
    Padding = 9,
    Stream = 10,
    ObjectIdsList = 11,
    ObjectIdsListSignature = 12,
    /// This flag is reserved. Do not use it.
    Target = 13,
    /// The buffer contains channel binding information.
    ChannelBindings = 14,
    /// The buffer contains a [DOMAIN_PASSWORD_INFORMATION](https://docs.microsoft.com/en-us/windows/win32/api/ntsecapi/ns-ntsecapi-domain_password_information) structure.
    ChangePasswordResponse = 15,
    /// The buffer specifies the [service principal name (SPN)](https://docs.microsoft.com/en-us/windows/win32/secgloss/s-gly) of the target.
    TargetHost = 16,
    /// The buffer contains an alert message.
    Alert = 17,
    /// The buffer contains a list of application protocol IDs, one list per application protocol negotiation extension type to be enabled.
    ApplicationProtocol = 18,
    /// The buffer contains a bitmask for a `ReadOnly` buffer.
    AttributeMark = 0xF000_0000,
    /// The buffer is read-only with no checksum. This flag is intended for sending header information to the security package for computing the checksum.
    /// The package can read this buffer, but cannot modify it.
    ReadOnly = 0x8000_0000,
    /// The buffer is read-only with a checksum.
    ReadOnlyWithChecksum = 0x1000_0000,
}

/// A flag that indicates how the credentials are used.
///
/// # MSDN
///
/// * [AcquireCredentialsHandleW function (fCredentialUse parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/nf-sspi-acquirecredentialshandlew)
#[derive(Debug, Copy, Clone, Eq, PartialEq, FromPrimitive, ToPrimitive)]
pub enum CredentialUse {
    Inbound = 1,
    Outbound = 2,
    Both = 3,
    Default = 4,
}

/// Represents the security principal in use.
#[derive(Debug, Clone)]
pub enum SecurityPackageType {
    Ntlm,
    Kerberos,
    Other(String),
}

impl string::ToString for SecurityPackageType {
    fn to_string(&self) -> String {
        match self {
            SecurityPackageType::Ntlm => ntlm::PKG_NAME.to_string(),
            SecurityPackageType::Kerberos => kerberos::PKG_NAME.to_string(),
            SecurityPackageType::Other(name) => name.clone(),
        }
    }
}

impl str::FromStr for SecurityPackageType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            ntlm::PKG_NAME => Ok(SecurityPackageType::Ntlm),
            kerberos::PKG_NAME => Ok(SecurityPackageType::Kerberos),
            s => Ok(SecurityPackageType::Other(s.to_string())),
        }
    }
}

/// General security principal information
///
/// Provides general information about a security package, such as its name and capabilities. Returned by `query_security_package_info`.
///
/// # MSDN
///
/// * [SecPkgInfoW structure](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secpkginfow)
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub capabilities: PackageCapabilities,
    pub rpc_id: u16,
    pub max_token_len: u32,
    pub name: SecurityPackageType,
    pub comment: String,
}

bitflags! {
    /// Set of bit flags that describes the capabilities of the security package. It is possible to combine them.
    ///
    /// # MSDN
    ///
    /// * [SecPkgInfoW structure (`fCapabilities` parameter)](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secpkginfow)
    pub struct PackageCapabilities: u32 {
        /// The security package supports the `make_signature` (TBI) and `verify_signature` (TBI) functions.
        const INTEGRITY = 0x1;
        /// The security package supports the `encrypt_message` and `decrypt_message` functions.
        const PRIVACY = 0x2;
        /// The package is interested only in the security-token portion of messages, and will ignore any other buffers. This is a performance-related issue.
        const TOKEN_ONLY = 0x4;
        /// Supports [datagram](https://docs.microsoft.com/en-us/windows/win32/secgloss/d-gly)-style authentication.
        /// For more information, see [SSPI Context Semantics](https://docs.microsoft.com/en-us/windows/win32/secauthn/sspi-context-semantics).
        const DATAGRAM = 0x8;
        /// Supports connection-oriented style authentication. For more information, see [SSPI Context Semantics](https://docs.microsoft.com/en-us/windows/win32/secauthn/sspi-context-semantics).
        const CONNECTION = 0x10;
        /// Multiple legs are required for authentication.
        const MULTI_REQUIRED = 0x20;
        /// Server authentication support is not provided.
        const CLIENT_ONLY = 0x40;
        /// Supports extended error handling. For more information, see [Extended Error Information](https://docs.microsoft.com/en-us/windows/win32/secauthn/extended-error-information).
        const EXTENDED_ERROR = 0x80;
        /// Supports Windows impersonation in server contexts.
        const IMPERSONATION = 0x100;
        /// Understands Windows principal and target names.
        const ACCEPT_WIN32_NAME = 0x200;
        /// Supports stream semantics. For more information, see [SSPI Context Semantics](https://docs.microsoft.com/en-us/windows/win32/secauthn/sspi-context-semantics).
        const STREAM = 0x400;
        /// Can be used by the [Microsoft Negotiate](https://docs.microsoft.com/windows/desktop/SecAuthN/microsoft-negotiate) security package.
        const NEGOTIABLE = 0x800;
        /// Supports GSS compatibility.
        const GSS_COMPATIBLE = 0x1000;
        /// Supports [LsaLogonUser](https://docs.microsoft.com/windows/desktop/api/ntsecapi/nf-ntsecapi-lsalogonuser).
        const LOGON = 0x2000;
        /// Token buffers are in ASCII characters format.
        const ASCII_BUFFERS = 0x4000;
        /// Supports separating large tokens into smaller buffers so that applications can make repeated calls to
        /// `initialize_security_context` and `accept_security_context` with the smaller buffers to complete authentication.
        const FRAGMENT = 0x8000;
        /// Supports mutual authentication.
        const MUTUAL_AUTH = 0x1_0000;
        /// Supports delegation.
        const DELEGATION = 0x2_0000;
        /// The security package supports using a checksum instead of in-place encryption when calling the `encrypt_message` function.
        const READONLY_WITH_CHECKSUM = 0x4_0000;
        /// Supports callers with restricted tokens.
        const RESTRICTED_TOKENS = 0x8_0000;
        /// The security package extends the [Microsoft Negotiate](https://docs.microsoft.com/windows/desktop/SecAuthN/microsoft-negotiate) security package.
        /// There can be at most one package of this type.
        const NEGO_EXTENDER = 0x10_0000;
        /// This package is negotiated by the package of type `NEGO_EXTENDER`.
        const NEGOTIABLE2 = 0x20_0000;
        /// This package receives all calls from app container apps.
        const APP_CONTAINER_PASSTHROUGH = 0x40_0000;
        /// This package receives calls from app container apps if one of the following checks succeeds:
        /// * Caller has default credentials capability
        /// * The target is a proxy server
        /// * The caller has supplied credentials
        const APP_CONTAINER_CHECKS = 0x80_0000;
    }
}

/// Indicates the sizes of important structures used in the message support functions.
/// `query_context_sizes` function returns this structure.
///
/// # MSDN
///
/// * [SecPkgContext_Sizes structure](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secpkgcontext_sizes)
#[derive(Debug, Clone)]
pub struct ContextSizes {
    pub max_token: u32,
    pub max_signature: u32,
    pub block: u32,
    pub security_trailer: u32,
}

/// Contains trust information about a certificate in a certificate chain,
/// summary trust information about a simple chain of certificates, or summary information about an array of simple chains.
/// `query_context_cert_trust_status` function returns this structure.
///
/// # MSDN
///
/// * [CERT_TRUST_STATUS structure](https://docs.microsoft.com/en-us/windows/win32/api/wincrypt/ns-wincrypt-cert_trust_status)
#[derive(Debug, Clone)]
pub struct CertTrustStatus {
    pub error_status: CertTrustErrorStatus,
    pub info_status: CertTrustInfoStatus,
}

bitflags! {
    /// Flags representing the error status codes used in `CertTrustStatus`.
    ///
    /// # MSDN
    ///
    /// * [CERT_TRUST_STATUS structure](https://docs.microsoft.com/en-us/windows/win32/api/wincrypt/ns-wincrypt-cert_trust_status)
    pub struct CertTrustErrorStatus: u32 {
        /// No error found for this certificate or chain.
        const NO_ERROR = 0x0;
        /// This certificate or one of the certificates in the certificate chain is not time valid.
        const IS_NOT_TIME_VALID = 0x1;
        const IS_NOT_TIME_NESTED = 0x2;
        /// Trust for this certificate or one of the certificates in the certificate chain has been revoked.
        const IS_REVOKED = 0x4;
        /// The certificate or one of the certificates in the certificate chain does not have a valid signature.
        const IS_NOT_SIGNATURE_VALID = 0x8;
        /// The certificate or certificate chain is not valid for its proposed usage.
        const IS_NOT_VALID_FOR_USAGE = 0x10;
        /// The certificate or certificate chain is based on an untrusted root.
        const IS_UNTRUSTED_ROOT = 0x20;
        /// The revocation status of the certificate or one of the certificates in the certificate chain is unknown.
        const REVOCATION_STATUS_UNKNOWN = 0x40;
        /// One of the certificates in the chain was issued by a
        /// [`certification authority`](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly)
        /// that the original certificate had certified.
        const IS_CYCLIC = 0x80;
        /// One of the certificates has an extension that is not valid.
        const INVALID_EXTENSION = 0x100;
        /// The certificate or one of the certificates in the certificate chain has a policy constraints extension,
        /// and one of the issued certificates has a disallowed policy mapping extension or does not have a
        /// required issuance policies extension.
        const INVALID_POLICY_CONSTRAINTS = 0x200;
        /// The certificate or one of the certificates in the certificate chain has a basic constraints extension,
        /// and either the certificate cannot be used to issue other certificates, or the chain path length has been exceeded.
        const INVALID_BASIC_CONSTRAINTS = 0x400;
        /// The certificate or one of the certificates in the certificate chain has a name constraints extension that is not valid.
        const INVALID_NAME_CONSTRAINTS = 0x800;
        /// The certificate or one of the certificates in the certificate chain has a name constraints extension that contains
        /// unsupported fields. The minimum and maximum fields are not supported.
        /// Thus minimum must always be zero and maximum must always be absent. Only UPN is supported for an Other Name.
        /// The following alternative name choices are not supported:
        /// * X400 Address
        /// * EDI Party Name
        /// * Registered Id
        const HAS_NOT_SUPPORTED_NAME_CONSTRAINT = 0x1000;
        /// The certificate or one of the certificates in the certificate chain has a name constraints extension and a name
        /// constraint is missing for one of the name choices in the end certificate.
        const HAS_NOT_DEFINED_NAME_CONSTRAINT = 0x2000;
        /// The certificate or one of the certificates in the certificate chain has a name constraints extension,
        /// and there is not a permitted name constraint for one of the name choices in the end certificate.
        const HAS_NOT_PERMITTED_NAME_CONSTRAINT = 0x4000;
        /// The certificate or one of the certificates in the certificate chain has a name constraints extension,
        /// and one of the name choices in the end certificate is explicitly excluded.
        const HAS_EXCLUDED_NAME_CONSTRAINT = 0x8000;
        /// The certificate chain is not complete.
        const IS_PARTIAL_CHAIN = 0x0001_0000;
        /// A [certificate trust list](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly)
        /// (CTL) used to create this chain was not time valid.
        const CTL_IS_NOT_TIME_VALID = 0x0002_0000;
        /// A CTL used to create this chain did not have a valid signature.
        const CTL_IS_NOT_SIGNATURE_VALID = 0x0004_0000;
        /// A CTL used to create this chain is not valid for this usage.
        const CTL_IS_NOT_VALID_FOR_USAGE = 0x0008_0000;
        /// The revocation status of the certificate or one of the certificates in the certificate chain is either offline or stale.
        const IS_OFFLINE_REVOCATION = 0x100_0000;
        /// The end certificate does not have any resultant issuance policies, and one of the issuing
        /// [certification authority](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly)
        /// certificates has a policy constraints extension requiring it.
        const NO_ISSUANCE_CHAIN_POLICY = 0x200_0000;
    }
}

bitflags! {
    /// Flags representing the info status codes used in `CertTrustStatus`.
    ///
    /// # MSDN
    ///
    /// * [CERT_TRUST_STATUS structure](https://docs.microsoft.com/en-us/windows/win32/api/wincrypt/ns-wincrypt-cert_trust_status)
    pub struct CertTrustInfoStatus: u32 {
        /// An exact match issuer certificate has been found for this certificate. This status code applies to certificates only.
        const HAS_EXACT_MATCH_ISSUER = 0x1;
        /// A key match issuer certificate has been found for this certificate. This status code applies to certificates only.
        const HAS_KEY_MATCH_ISSUER = 0x2;
        /// A name match issuer certificate has been found for this certificate. This status code applies to certificates only.
        const HAS_NAME_MATCH_ISSUER = 0x4;
        /// This certificate is self-signed. This status code applies to certificates only.
        const IS_SELF_SIGNED = 0x8;
        const AUTO_UPDATE_CA_REVOCATION = 0x10;
        const AUTO_UPDATE_END_REVOCATION = 0x20;
        const NO_OCSP_FAILOVER_TO_CRL = 0x40;
        const IS_KEY_ROLLOVER = 0x80;
        /// The certificate or chain has a preferred issuer. This status code applies to certificates and chains.
        const HAS_PREFERRED_ISSUER = 0x100;
        /// An issuance chain policy exists. This status code applies to certificates and chains.
        const HAS_ISSUANCE_CHAIN_POLICY = 0x200;
        /// A valid name constraints for all namespaces, including UPN. This status code applies to certificates and chains.
        const HAS_VALID_NAME_CONSTRAINTS = 0x400;
        /// This certificate is peer trusted. This status code applies to certificates only.
        const IS_PEER_TRUSTED = 0x800;
        /// This certificate's [certificate revocation list](https://docs.microsoft.com/windows/desktop/SecGloss/c-gly)
        /// (CRL) validity has been extended. This status code applies to certificates only.
        const HAS_CRL_VALIDITY_EXTENDED = 0x1000;
        const IS_FROM_EXCLUSIVE_TRUST_STORE = 0x2000;
        const IS_CA_TRUSTED = 0x4000;
        const HAS_AUTO_UPDATE_WEAK_SIGNATURE = 0x8000;
        const SSL_HANDSHAKE_OCSP = 0x0004_0000;
        const SSL_TIME_VALID_OCSP = 0x0008_0000;
        const SSL_RECONNECT_OCSP = 0x0010_0000;
        const IS_COMPLEX_CHAIN = 0x0001_0000;
        const HAS_ALLOW_WEAK_SIGNATURE = 0x0002_0000;
        const SSL_TIME_VALID = 0x100_0000;
        const NO_TIME_CHECK = 0x200_0000;
    }
}

/// Indicates the name of the user associated with a security context.
/// `query_context_names` function returns this structure.
///
/// # MSDN
///
/// * [SecPkgContext_NamesW structure](https://docs.microsoft.com/en-us/windows/win32/api/sspi/ns-sspi-secpkgcontext_namesw)
#[derive(Debug, Clone)]
pub struct ContextNames {
    pub username: String,
    pub domain: Option<String>,
}

/// The kind of an SSPI related error. Enables to specify an error based on its type.
#[repr(u32)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, FromPrimitive, ToPrimitive)]
pub enum ErrorKind {
    Unknown = 0,
    InsufficientMemory = 0x8009_0300,
    InvalidHandle = 0x8009_0301,
    UnsupportedFunction = 0x8009_0302,
    TargetUnknown = 0x8009_0303,
    /// May correspond to any internal error (I/O error, server error, etc.).
    InternalError = 0x8009_0304,
    SecurityPackageNotFound = 0x8009_0305,
    NotOwned = 0x8009_0306,
    CannotInstall = 0x8009_0307,
    /// Used in cases when supplied data is missing or invalid.
    InvalidToken = 0x8009_0308,
    CannotPack = 0x8009_0309,
    OperationNotSupported = 0x8009_030A,
    NoImpersonation = 0x8009_030B,
    LogonDenied = 0x8009_030C,
    UnknownCredentials = 0x8009_030D,
    NoCredentials = 0x8009_030E,
    /// Used in contexts of supplying invalid credentials.
    MessageAltered = 0x8009_030F,
    /// Used when a required NTLM state does not correspond to the current.
    OutOfSequence = 0x8009_0310,
    NoAuthenticatingAuthority = 0x8009_0311,
    BadPackageId = 0x8009_0316,
    ContextExpired = 0x8009_0317,
    IncompleteMessage = 0x8009_0318,
    IncompleteCredentials = 0x8009_0320,
    BufferTooSmall = 0x8009_0321,
    WrongPrincipalName = 0x8009_0322,
    TimeSkew = 0x8009_0324,
    UntrustedRoot = 0x8009_0325,
    IllegalMessage = 0x8009_0326,
    CertificateUnknown = 0x8009_0327,
    CertificateExpired = 0x8009_0328,
    EncryptFailure = 0x8009_0329,
    DecryptFailure = 0x8009_0330,
    AlgorithmMismatch = 0x8009_0331,
    SecurityQosFailed = 0x8009_0332,
    UnfinishedContextDeleted = 0x8009_0333,
    NoTgtReply = 0x8009_0334,
    NoIpAddress = 0x8009_0335,
    WrongCredentialHandle = 0x8009_0336,
    CryptoSystemInvalid = 0x8009_0337,
    MaxReferralsExceeded = 0x8009_0338,
    MustBeKdc = 0x8009_0339,
    StrongCryptoNotSupported = 0x8009_033A,
    TooManyPrincipals = 0x8009_033B,
    NoPaData = 0x8009_033C,
    PkInitNameMismatch = 0x8009_033D,
    SmartCardLogonRequired = 0x8009_033E,
    ShutdownInProgress = 0x8009_033F,
    KdcInvalidRequest = 0x8009_0340,
    KdcUnknownEType = 0x8009_0341,
    KdcUnknownEType2 = 0x8009_0342,
    UnsupportedPreAuth = 0x8009_0343,
    DelegationRequired = 0x8009_0345,
    BadBindings = 0x8009_0346,
    MultipleAccounts = 0x8009_0347,
    NoKerdKey = 0x8009_0348,
    CertWrongUsage = 0x8009_0349,
    DowngradeDetected = 0x8009_0350,
    SmartCardCertificateRevoked = 0x8009_0351,
    IssuingCAUntrusted = 0x8009_0352,
    RevocationOffline = 0x8009_0353,
    PkInitClientFailure = 0x8009_0354,
    SmartCardCertExpired = 0x8009_0355,
    NoS4uProtSupport = 0x8009_0356,
    CrossRealmDelegationFailure = 0x8009_0357,
    RevocationOfflineKdc = 0x8009_0358,
    IssuingCaUntrustedKdc = 0x8009_0359,
    KdcCertExpired = 0x8009_035A,
    KdcCertRevoked = 0x8009_035B,
    InvalidParameter = 0x8009_035D,
    DelegationPolicy = 0x8009_035E,
    PolicyNtlmOnly = 0x8009_035F,
    NoContext = 0x8009_0361,
    Pku2uCertFailure = 0x8009_0362,
    MutualAuthFailed = 0x8009_0363,
    OnlyHttpsAllowed = 0x8009_0365,
    ApplicationProtocolMismatch = 0x8009_0367,
}

/// Holds the `ErrorKind` and the description of the SSPI-related error.
#[derive(Debug, Clone)]
pub struct Error {
    pub error_type: ErrorKind,
    pub description: String,
}

/// The success status of SSPI-related operation.
#[derive(Debug, Copy, Clone, Eq, PartialEq, FromPrimitive, ToPrimitive)]
pub enum SecurityStatus {
    Ok = 0,
    ContinueNeeded = 0x0009_0312,
    CompleteNeeded = 0x0009_0313,
    CompleteAndContinue = 0x0009_0314,
    LocalLogon = 0x0009_0315,
    ContextExpired = 0x0009_0317,
    IncompleteCredentials = 0x0009_0320,
    Renegotiate = 0x0009_0321,
    NoLsaContext = 0x0009_0323,
}

impl Error {
    /// Allows to fill a new error easily, supplying it with a coherent description.
    pub fn new(error_type: ErrorKind, error: String) -> Self {
        Self {
            error_type,
            description: error,
        }
    }
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<Asn1DerError> for Error {
    fn from(err: Asn1DerError) -> Self {
        Self::new(ErrorKind::InvalidToken, format!("ASN1 DER error: {:?}", err))
    }
}

pub fn get_krb_status_from_code(error_code: &[u8]) -> &'static str {
    return match error_code {
        [0x0] => "KDC_ERR_NONE",
        [0x1] => "KDC_ERR_NAME_EXP",
        [0x2] => "KDC_ERR_SERVICE_EXP",
        [0x3] => "KDC_ERR_BAD_PVNO",
        [0x4] => "KDC_ERR_C_OLD_MAST_KVNO",
        [0x5] => "KDC_ERR_S_OLD_MAST_KVNO",
        [0x6] => "Unrecognised Username - KDC_ERR_C_PRINCIPAL_UNKNOWN",
        [0x7] => "Unrecognised Server - KDC_ERR_S_PRINCIPAL_UNKNOWN",
        [0x8] => "KDC_ERR_PRINCIPAL_NOT_UNIQUE",
        [0x9] => "KDC_ERR_NULL_KEY",
        [0xA] => "KDC_ERR_CANNOT_POSTDATE",
        [0xB] => "KDC_ERR_NEVER_VALID",
        [0xC] => "KDC_ERR_POLICY",
        [0xD] => "KDC_ERR_BADOPTION",
        [0xE] => "KDC_ERR_ETYPE_NOTSUPP",
        [0xF] => "KDC_ERR_SUMTYPE_NOSUPP",
        [0x10] => "KDC_ERR_PADATA_TYPE_NOSUPP",
        [0x11] => "KDC_ERR_TRTYPE_NO_SUPP",
        [0x12] => "KDC_ERR_CLIENT_REVOKED",
        [0x13] => "KDC_ERR_SERVICE_REVOKED",
        [0x14] => "KDC_ERR_TGT_REVOKED",
        [0x15] => "KDC_ERR_CLIENT_NOTYET",
        [0x16] => "KDC_ERR_SERVICE_NOTYET",
        [0x17] => "Credentials Expired - KDC_ERR_KEY_EXPIRED",
        [0x18] => "Incorrect Credentials - KDC_ERR_PREAUTH_FAILED",
        [0x19] => "KDC_ERR_PREAUTH_REQUIRED",
        [0x1A] => "KDC_ERR_SERVER_NOMATCH",
        [0x1B] => "KDC_ERR_SVC_UNAVAILABLE",
        [0x1F] => "KRB_AP_ERR_BAD_INTEGRITY",
        [0x20] => "KRB_AP_ERR_TKT_EXPIRED",
        [0x21] => "KRB_AP_ERR_TKT_NYV",
        [0x22] => "KRB_AP_ERR_REPEAT",
        [0x23] => "KRB_AP_ERR_NOT_US",
        [0x24] => "KRB_AP_ERR_BADMATCH",
        [0x25] => "KRB_AP_ERR_SKEW",
        [0x26] => "KRB_AP_ERR_BADADDR",
        [0x27] => "KRB_AP_ERR_BADVERSION",
        [0x28] => "KRB_AP_ERR_MSG_TYPE",
        [0x29] => "KRB_AP_ERR_MODIFIED",
        [0x2A] => "KRB_AP_ERR_BADORDER",
        [0x2C] => "KRB_AP_ERR_BADKEYVER",
        [0x2D] => "KRB_AP_ERR_NOKEY",
        [0x2E] => "KRB_AP_ERR_MUT_FAIL",
        [0x2F] => "KRB_AP_ERR_BADDIRECTION",
        [0x30] => "KRB_AP_ERR_METHOD",
        [0x31] => "KRB_AP_ERR_BADSEQ",
        [0x32] => "KRB_AP_ERR_INAPP_CKSUM",
        [0x33] => "KRB_AP_PATH_NOT_ACCEPTED",
        [0x34] => "KRB_ERR_RESPONSE_TOO_BIG",
        [0x3C] => "KRB_ERR_GENERIC",
        [0x3D] => "KRB_ERR_FIELD_TOOLONG",
        [0x3E] => "KDC_ERR_CLIENT_NOT_TRUSTED",
        [0x3F] => "KDC_ERR_KDC_NOT_TRUSTED",
        [0x40] => "KDC_ERR_INVALID_SIG",
        [0x41] => "KDC_ERR_KEY_TOO_WEAK",
        [0x42] => "KRB_AP_ERR_USER_TO_USER_REQUIRED",
        [0x43] => "KRB_AP_ERR_NO_TGT",
        [0x44] => "Unrecognised Domain - KDC_ERR_WRONG_REALM",
        _ =>  "MISSING_ERROR",
    }
}

impl From<KrbError> for Error {
    fn from(err: KrbError) -> Self {
        let error_code = err.0.error_code.as_unsigned_bytes_be();
        let error = get_krb_status_from_code(error_code);

        Self::new(
            ErrorKind::InternalError,
            format!("Got the krb error: {} ({})", error, err.0.to_string()),
        )
    }
}

impl From<kerberos_crypto::Error> for Error {
    fn from(err: kerberos_crypto::Error) -> Self {
        use kerberos_crypto::Error;

        match err {
            Error::DecryptionError(description) => Self {
                error_type: ErrorKind::DecryptFailure,
                description,
            },
            Error::UnsupportedAlgorithm(alg) => Self {
                error_type: ErrorKind::InternalError,
                description: format!("unsupported algorithm: {}", alg),
            },
            Error::InvalidKeyCharset => Self {
                error_type: ErrorKind::InternalError,
                description: "invalid key charset".to_owned(),
            },
            Error::InvalidKeyLength(len) => Self {
                error_type: ErrorKind::InternalError,
                description: format!("invalid key len: {}", len),
            },
        }
    }
}

impl From<CharSetError> for Error {
    fn from(err: CharSetError) -> Self {
        Self {
            error_type: ErrorKind::InternalError,
            description: err.to_string(),
        }
    }
}

impl From<GssApiMessageError> for Error {
    fn from(err: GssApiMessageError) -> Self {
        match err {
            GssApiMessageError::IoError(err) => Self::from(err),
            GssApiMessageError::InvalidId(_, _) => Self {
                error_type: ErrorKind::InvalidToken,
                description: err.to_string(),
            },
            GssApiMessageError::InvalidMicFiller(_) => Self {
                error_type: ErrorKind::InvalidToken,
                description: err.to_string(),
            },
            GssApiMessageError::InvalidWrapFiller(_) => Self {
                error_type: ErrorKind::InvalidToken,
                description: err.to_string(),
            },
            GssApiMessageError::Asn1Error(_) => Self {
                error_type: ErrorKind::InvalidToken,
                description: err.to_string(),
            },
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::new(ErrorKind::InternalError, format!("IO error: {:?}", err))
    }
}

impl From<rand::Error> for Error {
    fn from(err: rand::Error) -> Self {
        Self::new(ErrorKind::InternalError, format!("Rand error: {:?}", err))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::new(ErrorKind::InternalError, format!("UTF-8 error: {:?}", err))
    }
}

impl From<string::FromUtf16Error> for Error {
    fn from(err: string::FromUtf16Error) -> Self {
        Self::new(ErrorKind::InternalError, format!("UTF-16 error: {:?}", err))
    }
}

impl From<Error> for io::Error {
    fn from(err: Error) -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            format!("{:?}: {}", err.error_type, err.description),
        )
    }
}
