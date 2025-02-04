use std::marker::PhantomData;

use chrono::NaiveDateTime;

use super::{
    ToAssign, WithContextRequirements, WithCredentialsHandle, WithOutput, WithTargetDataRepresentation,
    WithoutContextRequirements, WithoutCredentialsHandle, WithoutOutput, WithoutTargetDataRepresentation,
};
use crate::sspi::internal::SspiImpl;
use crate::sspi::{self, DataRepresentation, SecurityBuffer, SecurityStatus, ServerRequestFlags, ServerResponseFlags};

pub type EmptyAcceptSecurityContext<'a, I, C> = AcceptSecurityContext<
    'a,
    I,
    C,
    WithoutCredentialsHandle,
    WithoutContextRequirements,
    WithoutTargetDataRepresentation,
    WithoutOutput,
>;
pub type FilledAcceptSecurityContext<'a, I, C> = AcceptSecurityContext<
    'a,
    I,
    C,
    WithCredentialsHandle,
    WithContextRequirements,
    WithTargetDataRepresentation,
    WithOutput,
>;

/// Contains data returned by calling the `execute` method of
/// the `AcceptSecurityContextBuilder` structure. The builder is returned by calling
/// the `accept_security_context` method.
#[derive(Debug, Clone)]
pub struct AcceptSecurityContextResult {
    pub status: SecurityStatus,
    pub flags: ServerResponseFlags,
    pub expiry: Option<NaiveDateTime>,
}

/// A builder to execute one of the SSPI functions. Returned by the `accept_security_context` method.
///
/// # Requirements for execution
///
/// These methods are required to be called before calling the `execute` method
/// * [`with_credentials_handle`](struct.AcceptSecurityContext.html#method.with_credentials_handle)
/// * [`with_context_requirements`](struct.AcceptSecurityContext.html#method.with_context_requirements)
/// * [`with_target_data_representation`](struct.AcceptSecurityContext.html#method.with_target_data_representation)
/// * [`with_output`](struct.AcceptSecurityContext.html#method.with_output)
#[derive(Debug)]
pub struct AcceptSecurityContext<
    'a,
    Inner,
    CredsHandle,
    CredsHandleSet,
    ContextRequirementsSet,
    TargetDataRepresentationSet,
    OutputSet,
> where
    Inner: SspiImpl,
    CredsHandleSet: ToAssign,
    ContextRequirementsSet: ToAssign,
    TargetDataRepresentationSet: ToAssign,
    OutputSet: ToAssign,
{
    inner: Option<&'a mut Inner>,
    phantom_creds_use_set: PhantomData<CredsHandleSet>,
    phantom_context_req_set: PhantomData<ContextRequirementsSet>,
    phantom_data_repr_set: PhantomData<TargetDataRepresentationSet>,
    phantom_output_set: PhantomData<OutputSet>,

    pub credentials_handle: Option<&'a mut CredsHandle>,
    pub context_requirements: ServerRequestFlags,
    pub target_data_representation: DataRepresentation,
    pub output: &'a mut [SecurityBuffer],

    pub input: Option<&'a mut [SecurityBuffer]>,
}

impl<
        'a,
        Inner: SspiImpl,
        CredsHandle,
        CredsHandleSet: ToAssign,
        ContextRequirementsSet: ToAssign,
        TargetDataRepresentationSet: ToAssign,
        OutputSet: ToAssign,
    >
    AcceptSecurityContext<
        'a,
        Inner,
        CredsHandle,
        CredsHandleSet,
        ContextRequirementsSet,
        TargetDataRepresentationSet,
        OutputSet,
    >
{
    pub(crate) fn new(inner: &'a mut Inner) -> Self {
        Self {
            inner: Some(inner),
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: None,
            context_requirements: ServerRequestFlags::empty(),
            target_data_representation: DataRepresentation::Network,

            output: &mut [],
            input: None,
        }
    }

    /// Specifies the server credentials. To retrieve this handle, the server calls the `acquire_credentials_handle`
    /// method with either the `CredentialUse::Inbound` or `CredentialUse::Outbound` flag set.
    pub fn with_credentials_handle(
        self,
        credentials_handle: &'a mut CredsHandle,
    ) -> AcceptSecurityContext<
        'a,
        Inner,
        CredsHandle,
        WithCredentialsHandle,
        ContextRequirementsSet,
        TargetDataRepresentationSet,
        OutputSet,
    > {
        AcceptSecurityContext {
            inner: self.inner,
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: Some(credentials_handle),
            context_requirements: self.context_requirements,
            target_data_representation: self.target_data_representation,
            output: self.output,

            input: self.input,
        }
    }

    /// Specifies bit flags that specify the attributes required by the server to establish the context.
    pub fn with_context_requirements(
        self,
        context_requirements: ServerRequestFlags,
    ) -> AcceptSecurityContext<
        'a,
        Inner,
        CredsHandle,
        CredsHandleSet,
        WithContextRequirements,
        TargetDataRepresentationSet,
        OutputSet,
    > {
        AcceptSecurityContext {
            inner: self.inner,
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: self.credentials_handle,
            context_requirements,
            target_data_representation: self.target_data_representation,
            output: self.output,

            input: self.input,
        }
    }

    /// Specifies the data representation, such as byte ordering, on the target.
    pub fn with_target_data_representation(
        self,
        target_data_representation: DataRepresentation,
    ) -> AcceptSecurityContext<
        'a,
        Inner,
        CredsHandle,
        CredsHandleSet,
        ContextRequirementsSet,
        WithTargetDataRepresentation,
        OutputSet,
    > {
        AcceptSecurityContext {
            inner: self.inner,
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: self.credentials_handle,
            context_requirements: self.context_requirements,
            target_data_representation,
            output: self.output,

            input: self.input,
        }
    }

    /// Specifies a mutable reference to a buffer with `SecurityBuffer` that contains the output buffer descriptor.
    /// This buffer is sent to the client for input into additional calls to `initialize_security_context`. An output
    /// buffer may be generated even if the function returns `SecurityStatus::Ok`. Any buffer generated must be sent
    /// back to the client application.
    pub fn with_output(
        self,
        output: &'a mut [SecurityBuffer],
    ) -> AcceptSecurityContext<
        'a,
        Inner,
        CredsHandle,
        CredsHandleSet,
        ContextRequirementsSet,
        TargetDataRepresentationSet,
        WithOutput,
    > {
        AcceptSecurityContext {
            inner: self.inner,
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: self.credentials_handle,
            context_requirements: self.context_requirements,
            target_data_representation: self.target_data_representation,
            output,

            input: self.input,
        }
    }

    /// Specifies a mutable reference to a `SecurityBuffer` generated by a client call to `initialize_security_context`.
    /// The structure contains the input buffer descriptor.
    pub fn with_input(self, input: &'a mut [SecurityBuffer]) -> Self {
        Self {
            input: Some(input),
            ..self
        }
    }
}

impl<'a, Inner: SspiImpl<CredentialsHandle = CredsHandle>, CredsHandle>
    FilledAcceptSecurityContext<'a, Inner, CredsHandle>
{
    /// Executes the SSPI function that the builder represents.
    pub fn execute(mut self) -> sspi::Result<AcceptSecurityContextResult> {
        let inner = self.inner.take().unwrap();
        inner.accept_security_context_impl(self)
    }

    pub(crate) fn transform<Inner2>(self, inner: &'a mut Inner2) -> FilledAcceptSecurityContext<'a, Inner2, CredsHandle>
    where
        Inner2: SspiImpl,
    {
        AcceptSecurityContext {
            inner: Some(inner),
            phantom_creds_use_set: PhantomData,
            phantom_context_req_set: PhantomData,
            phantom_data_repr_set: PhantomData,
            phantom_output_set: PhantomData,

            credentials_handle: self.credentials_handle,
            context_requirements: self.context_requirements,
            target_data_representation: self.target_data_representation,

            output: self.output,
            input: self.input,
        }
    }
}
