use crate::clients::PlainHttpAuth;
use crate::errors::Error;
use actix::{Actor, Addr};
use actix_web::client::{self, ClientConnector};
use actix_web::HttpMessage;
use futures::Future;
use log::{debug, error};
use serde::Deserialize;
use serde_json::from_slice;
use std::str::from_utf8;
use std::time::Duration;

const CHAIN_OUTPUTS_BY_HEIGHT: &'static str = "v1/chain/outputs/byheight";

#[derive(Clone)]
pub struct Node {
    conn: Addr<ClientConnector>,
    username: String,
    password: String,
    url: String,
}

impl Node {
    pub fn new(url: &str, username: &str, password: &str) -> Self {
        let connector = ClientConnector::default()
            .conn_lifetime(Duration::from_secs(300))
            .conn_keep_alive(Duration::from_secs(300));
        Node {
            url: url.trim_end_matches('/').to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
            conn: connector.start(),
        }
    }

    pub fn blocks(&self, start: i64, end: i64) -> impl Future<Item = Vec<Block>, Error = Error> {
        let url = format!(
            "{}/{}?start_height={}&end_height={}",
            self.url, CHAIN_OUTPUTS_BY_HEIGHT, start, end
        );
        debug!("Get latest blocks from node {}", url);
        client::get(&url) // <- Create request builder
            .auth(&self.username, &self.password)
            .finish()
            .unwrap()
            .send() // <- Send http request
            .map_err(|e| Error::NodeAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::NodeAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(resp)
                }
            })
            .and_then(|resp| {
                // <- server http response
                resp.body()
                    .limit(10 * 1024 * 1024)
                    .map_err(|e| Error::NodeAPIError(s!(e)))
                    .and_then(move |bytes| {
                        let blocks: Vec<Block> = from_slice(&bytes).map_err(|e| {
                            error!(
                                "Cannot decode json {:?}:\n with error {} ",
                                from_utf8(&bytes),
                                e
                            );
                            Error::NodeAPIError(format!("Cannot decode json {}", e))
                        })?;
                        Ok(blocks)
                    })
            })
    }
}

#[derive(Deserialize, Debug)]
pub struct Block {
    pub header: Header,
    pub outputs: Vec<Output>,
}

#[derive(Deserialize, Debug)]
pub struct Header {
    pub height: u64,
}

#[derive(Deserialize, Debug)]
pub struct Output {
    pub output_type: String,
    pub commit: String,
    pub block_height: Option<u64>,
}

impl Output {
    pub fn is_coinbase(&self) -> bool {
        self.output_type == "Coinbase"
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const _SAMPLE: &'static str = r#"
 {
    "header": {
      "hash": "077360fcf848b71c8c07bb35fd361ad8aa5b9608cd62130a1b5a50d8c071f091",
      "height": 84586,
      "previous": "1d4ab68f8d7ae4e32116b0a84a2046a1f737f1f8431aa7b934248b4a58a0b14c"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "0808d9594ac88429049238fcce5bf69944e4c05f87ad318c56b776076de630d46c",
        "spent": false,
        "proof": null,
        "proof_hash": "16519ea0616790dbe3c7acc6f3e40aa0e776106cad02e20f94a003a646d55bd9",
        "block_height": 84586,
        "merkle_proof": "0000000000044ddc000000000000000a60d7b823fd9344019c69c29b4b027818e4722e1454e716c88ddd999207c7f22ca60c549e1a0e5f10bf9048c222031c90a72ac0916bc40550034268afaddf255fce7a670028038f8fb647a8d4c373254d0b0e6b0fb9dfcff4e43dda86b294833f1cdebaec85c75b0c630528d8360689893f2557fe47dbf5c667ebb5f2647803269a38194d07acd484e93e847fe5292c09c2a7499195357c65f64bf09374e85ef4fea77649aa53801ccd7b7ccff258aa08f9b16951a2098c6539bea0269ea72b0d2c0d669579504dbd1788ee804e487cf86898199cb3a881467bec682fa7e38c9b5bc2db1dff1d1b3f0851a4323063aca898c251f8f3ab6b1ce8dd7f2a4279cf341f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282073
      },
      {
        "output_type": "Transaction",
        "commit": "08411bb50f21a1d154687cda50410473ce0fe6bc723c6c76dae5a28eea4a189ea6",
        "spent": false,
        "proof": null,
        "proof_hash": "f1768ef908bae2d170e57b0ca064a40647eeb5e8b07e4fd0cd74ce8ba205378f",
        "block_height": 84586,
        "merkle_proof": null,
        "mmr_index": 282074
      },
            {
        "output_type": "Transaction",
        "commit": "093ae209d95233eefee1a97ce27126cd2f3f41e17662e6f79386729ba07125cd7c",
        "spent": false,
        "proof": null,
        "proof_hash": "6406260bbe91ff428f94f5bbc9f9da2b8942f7305c822430a6a034265329b8c8",
        "block_height": 84586,
        "merkle_proof": null,
        "mmr_index": 282076
      }
    ]
  }"#;

    const SAMPLE2: &'static str = r#"
    [
  {
    "header": {
      "hash": "10c8c0761956d260c3a434868e150e6f6564c7296f06d401a943b61f909ba89f",
      "height": 85025,
      "previous": "26100a2992265396f988b33ce3d62a6da4a79b7956b64b5b381c0b1b81559677"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08318677b505a27a004e703b09a55adb3a92cea579942a2553ef593f7242d7df4b",
        "spent": false,
        "proof": null,
        "proof_hash": "e2e2db23338164ad50107096ab81d06ead10282806a51c46efa01b8a13c74695",
        "block_height": 85025,
        "merkle_proof": "000000000004516c00000000000000088782f8f5ed7e2a3690eeac3561002b7078522a723740167d0edb9f491933b900848771b0f258b5393f5220c118c7cd244707cdbcb2721d760fd0be97dbc516788f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282987
      }
    ]
  },
  {
    "header": {
      "hash": "26100a2992265396f988b33ce3d62a6da4a79b7956b64b5b381c0b1b81559677",
      "height": 85024,
      "previous": "0da6e642a221c440e65c9746fc08763b1c7a2946bab85fe8ef8873c760b84348"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "09a361bdb398b775801e8dc29c6eb524617ff0fbbfe3f56e0a606f5e458ab7e8c7",
        "spent": false,
        "proof": null,
        "proof_hash": "9a514b8ed96bf7da4afb0b708135fc22c424a37d675a58b2e01137e012ccece0",
        "block_height": 85024,
        "merkle_proof": "000000000004516a0000000000000007848771b0f258b5393f5220c118c7cd244707cdbcb2721d760fd0be97dbc516788f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282986
      }
    ]
  },
  {
    "header": {
      "hash": "0da6e642a221c440e65c9746fc08763b1c7a2946bab85fe8ef8873c760b84348",
      "height": 85023,
      "previous": "0fffd964ff7ec1cb157f2a214ef0732696153e71cdc857a996703c90eaf49228"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08f328dc9951359d668b28029dde1bd0c4dbf54ee81be3bf2ac77e33fa950e46a1",
        "spent": false,
        "proof": null,
        "proof_hash": "fc94c88164e3aedede495e70056a87d9987ee3653c9fd627676433bbf944a65c",
        "block_height": 85023,
        "merkle_proof": "000000000004516900000000000000092cb00b595ff02c64e0403a35b42ea93123af32933089d28a30d071129596c776d329972693e89794b9cd7351c394241cfef240b0b21a3b5b4598b3e4f72376315afbbfd4c0a179fdbe23ff160603f0d5a3069e127ddd6d2cbf759014792d7eef8f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282982
      }
    ]
  },
  {
    "header": {
      "hash": "0fffd964ff7ec1cb157f2a214ef0732696153e71cdc857a996703c90eaf49228",
      "height": 85022,
      "previous": "00a9fd33bd2f56c7b075d2933502af11d91c05439b33716ef3cf8aac6aedd98a"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "098dd5694666738814d5306523750eaae21514736014a6087ea7db12f28dd6ef2f",
        "spent": false,
        "proof": null,
        "proof_hash": "f3737bf54ad8861374dfb92a4c81f8040d9e525792441bb6593cd8c1782a9c27",
        "block_height": 85022,
        "merkle_proof": "00000000000451650000000000000008d329972693e89794b9cd7351c394241cfef240b0b21a3b5b4598b3e4f72376315afbbfd4c0a179fdbe23ff160603f0d5a3069e127ddd6d2cbf759014792d7eef8f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282981
      }
    ]
  },
  {
    "header": {
      "hash": "00a9fd33bd2f56c7b075d2933502af11d91c05439b33716ef3cf8aac6aedd98a",
      "height": 85021,
      "previous": "02601c825650a9a3e6e3e28ec1f96ca27d06909a6968f61fc36a0650fb3799cd"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08295a570c6e34a1226575035e388e86e8158cfab63bba0c4a4b8de35837a79126",
        "spent": false,
        "proof": null,
        "proof_hash": "b967d14809c1839689f78c7fd2138a20a82013cc019faabdf261ffc1dd57f504",
        "block_height": 85021,
        "merkle_proof": "00000000000451640000000000000008af294e24f2950528ef110c1f021828d2ac0bc61899c45f04ec891eae0713e4605afbbfd4c0a179fdbe23ff160603f0d5a3069e127ddd6d2cbf759014792d7eef8f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282979
      }
    ]
  },
  {
    "header": {
      "hash": "02601c825650a9a3e6e3e28ec1f96ca27d06909a6968f61fc36a0650fb3799cd",
      "height": 85020,
      "previous": "00491073bbffe7f7cd9d6027fa7e971c1a15591338b3febd0797b5a468baa710"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08dd8c0eda754998149f291e805491b130db102626add5939dbd97abda00e3324c",
        "spent": false,
        "proof": null,
        "proof_hash": "6289041da9408768a1e3ac87c992848354427fb359829ff1bbaa31d5b0926ac0",
        "block_height": 85020,
        "merkle_proof": "000000000004516200000000000000075afbbfd4c0a179fdbe23ff160603f0d5a3069e127ddd6d2cbf759014792d7eef8f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282978
      }
    ]
  },
  {
    "header": {
      "hash": "00491073bbffe7f7cd9d6027fa7e971c1a15591338b3febd0797b5a468baa710",
      "height": 85019,
      "previous": "06db07394f27e278cfc9b655319ce32d8dbc40156bbfd8075b65327ac1f8477f"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08e9b2847b6750405b6ced03f8e68f0b1cd1ba8886a95aecf609d24f25533bea02",
        "spent": false,
        "proof": null,
        "proof_hash": "ec3b99439eabba2876902dae89a7a8e4d8c64e1791c52c3c6ab76813972e33bd",
        "block_height": 85019,
        "merkle_proof": "00000000000451610000000000000008d307582b64140077ac623e943c7c4d492f491493f94da2c3f4c76b8859ff2615012c2d2b705147367b5708b7b9285c73c81ffb9fd9f5b04895480748b1da6de48f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282975
      }
    ]
  },
  {
    "header": {
      "hash": "06db07394f27e278cfc9b655319ce32d8dbc40156bbfd8075b65327ac1f8477f",
      "height": 85018,
      "previous": "1bee748a9266c553a99eec58ff62f423d70353a436bf1df648fc5d8b35e14f73"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "081a4c23db3ca1a246f7a0f3ce012cea9d2219658f372515dd9886cfecce7ecf1f",
        "spent": false,
        "proof": null,
        "proof_hash": "2a1d59f40e1200be9c653d7fd1811a532a40e3fd1d98b1f5ed15b1891e845859",
        "block_height": 85018,
        "merkle_proof": "000000000004515e0000000000000008fbcbcaa1b14d47fedecbf3680cfa763763a38e97b4c9b6da3e25050376eb21a3d307582b64140077ac623e943c7c4d492f491493f94da2c3f4c76b8859ff26158f4c1dcb056fb5e73739db16e53a6414f5ab88472d703a014edd53747b6f64ba25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282971
      },
      {
        "output_type": "Transaction",
        "commit": "0967d1a907613beea587e1cfd78d1875bbf00e3af547a634e86867b172620ce0f7",
        "spent": false,
        "proof": null,
        "proof_hash": "21b9a789a9aef02f90255ab40208c107ac46c298fa8749866ddadc1bdbb08256",
        "block_height": 85018,
        "merkle_proof": null,
        "mmr_index": 282972
      },
      {
        "output_type": "Transaction",
        "commit": "08e5a229a39df9abd6f8f7b481dbf883b15369916e4b08949dd285255258024158",
        "spent": true,
        "proof": null,
        "proof_hash": "398a44a567b275cd5da13e2b6ef328dbb444376f31631c65c18b1a39acddeaad",
        "block_height": null,
        "merkle_proof": null,
        "mmr_index": 282974
      }
    ]
  },
  {
    "header": {
      "hash": "1bee748a9266c553a99eec58ff62f423d70353a436bf1df648fc5d8b35e14f73",
      "height": 85017,
      "previous": "22d3502f373016e17587563af0c22dd2b147de01390d3bcbfc958f1fb9c0f370"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08e35f27d3fc833f38ec3fda582fe401d03539ef8a6dfb20d7ec06ef29a6fcc0e8",
        "spent": false,
        "proof": null,
        "proof_hash": "8cd8b25d8c85c6c37fd8c3653a516ec3a1be4ff83fa52685c4d384f6a022014b",
        "block_height": 85017,
        "merkle_proof": "000000000004515a000000000000000961e93ad98464565b852c95a898ee10fcf87d51e964a6d15eef00de99bb2f8282602163f7ee2f1ababc4fe49d9023c1dfce338333aa28b0c08279763cc443bb83ef58de054ec17f96476e2f1b34f9cf0e00ce7deb4afc6cb1acdb3afe44029623f175b4e538fdab77dc568b21688343c64c6956d9af8e6ec195080981d167350d25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282966
      }
    ]
  },
  {
    "header": {
      "hash": "22d3502f373016e17587563af0c22dd2b147de01390d3bcbfc958f1fb9c0f370",
      "height": 85016,
      "previous": "251adc13a295955984cf50319b7a4b3426b0e708bf7ab3b08faac1f4f14ea754"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "09f01d76da4740894defc6b035c65440d1d1e5a2c3e69ea25f51cdc15ead5d6160",
        "spent": false,
        "proof": null,
        "proof_hash": "b7658912e69f0a4b86a7194e1e9fe38ac05f42574f91f22aed9250ea74efef4a",
        "block_height": 85016,
        "merkle_proof": "00000000000451550000000000000008602163f7ee2f1ababc4fe49d9023c1dfce338333aa28b0c08279763cc443bb83ef58de054ec17f96476e2f1b34f9cf0e00ce7deb4afc6cb1acdb3afe44029623f175b4e538fdab77dc568b21688343c64c6956d9af8e6ec195080981d167350d25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282965
      }
    ]
  },
  {
    "header": {
      "hash": "251adc13a295955984cf50319b7a4b3426b0e708bf7ab3b08faac1f4f14ea754",
      "height": 85015,
      "previous": "26d8a54c37fafa89e210db60b0acb7653f26768987038f54416b7d6dbc2031fa"
    },
    "outputs": [
      {
        "output_type": "Coinbase",
        "commit": "08406b933b938626ee62c11db3bd3e6a82eed399b55c118afcada74ede3d872467",
        "spent": false,
        "proof": null,
        "proof_hash": "1f4ed2ec56b0b0169a6fa2aeecf763b0cb4dd951ffc2554a36728b348b9a5448",
        "block_height": 85015,
        "merkle_proof": "00000000000451540000000000000008c4b54462e11ab0a184480a016f3a49078b6c0e6bd3392037784290453b1e2d59ef58de054ec17f96476e2f1b34f9cf0e00ce7deb4afc6cb1acdb3afe44029623f175b4e538fdab77dc568b21688343c64c6956d9af8e6ec195080981d167350d25fc2d784bf643ec67c362f584f397d8515a2ee70c0cbbbc43446c86426add5095c899e3570555ab128a1aaaffa25439429638169d37a4d6387b7ac092d6f2a09f32ec4bf001a2696a7995d399339be6dcbc87488dc1ebd9cac82823208849c21f3c2c3e9f6b23a2b49349964a959a8a4e33e1549194d721add42eba8c4a134758661f2b62fa23eb399def5cec17297cf5d612d3186deb30f598ae78d874dc13",
        "mmr_index": 282963
      }
    ]
  }
  ]
"#;

    #[test]
    fn blocks_load_test() {
        match from_slice::<Vec<Block>>(SAMPLE2.as_bytes()) {
            Ok(_) => (),
            Err(_) => assert!(false),
        }
    }
}
