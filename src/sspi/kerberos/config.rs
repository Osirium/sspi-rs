use std::env;
use std::fmt::Debug;
use std::str::FromStr;

use url::Url;

#[cfg(feature = "network_client")]
use super::network_client::reqwest_network_client::ReqwestNetworkClient;
use super::SSPI_KDC_URL_ENV;

#[derive(Debug, Clone)]
pub enum KdcType {
    Kdc,
    KdcProxy,
}

#[derive(Debug)]
pub struct KerberosConfig {
    pub url: Url,
    pub kdc_type: KdcType,
    pub network_client: Box<ReqwestNetworkClient>,
}

impl KerberosConfig {
    pub fn get_kdc_env() -> Option<(Url, KdcType)> {
        let mut kdc_url_env = env::var(SSPI_KDC_URL_ENV).expect("SSPI_KDC_URL environment variable must be set!");
        if !kdc_url_env.contains("://") {
            kdc_url_env = format!("tcp://{}", kdc_url_env);
        }
        let kdc_url = Url::from_str(&kdc_url_env).unwrap();
        let kdc_type = match kdc_url.scheme() {
            "tcp" => KdcType::Kdc,
            "udp" => KdcType::Kdc,
            "http" => KdcType::KdcProxy,
            "https" => KdcType::KdcProxy,
            _ => KdcType::Kdc,
        };
        Some((kdc_url, kdc_type))
    }

    pub fn new_with_network_client(network_client: Box<ReqwestNetworkClient>) -> Self {
        if let Some((kdc_url, kdc_type)) = Self::get_kdc_env() {
            Self {
                url: kdc_url,
                kdc_type,
                network_client,
            }
        } else {
            panic!("{} environment variable is not set properly!", SSPI_KDC_URL_ENV);
        }
    }

    #[cfg(feature = "network_client")]
    pub fn from_env() -> Self {
        let network_client = Box::new(ReqwestNetworkClient::new());
        Self::new_with_network_client(network_client)
    }

    #[cfg(not(feature = "network_client"))]
    pub fn from_env(network_client: Box<ReqwestNetworkClient>) -> Self {
        Self::new_with_network_client(network_client)
    }
}

impl Clone for KerberosConfig {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            kdc_type: self.kdc_type.clone(),
            network_client: self.network_client.clone(),
        }
    }
}
