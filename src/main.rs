use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt, lock};
use league_client_connector::LeagueClientConnector;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::HeaderValue,
    Message
};
use anyhow::{Result};
use serde_json::Value;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use url::Url;
use reqwest::{header, Certificate};
use chrono::Utc;

#[derive(Serialize, Deserialize, Debug)]
pub struct TeamElement {
    #[serde(rename = "assignedPosition")]
    assigned_position: String,
    #[serde(rename = "cellId")]
    cell_id: i64,
    #[serde(rename = "championId")]
    champion_id: i64,
    #[serde(rename = "championPickIntent")]
    champion_pick_intent: i64,
    #[serde(rename = "entitledFeatureType")]
    entitled_feature_type: String,
    #[serde(rename = "selectedSkinId")]
    selected_skin_id: i64,
    #[serde(rename = "spell1Id")]
    spell1_id: f64,
    #[serde(rename = "spell2Id")]
    spell2_id: f64,
    #[serde(rename = "summonerId")]
    summoner_id: i64,
    team: i64,
    #[serde(rename = "wardSkinId")]
    ward_skin_id: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let lockfile = LeagueClientConnector::parse_lockfile().expect("The client is not opened!");
    let mut url = format!("wss://127.0.0.1:{}", lockfile.port).into_client_request()?;

    let auth_token = encode_token(&lockfile.password);
    let cert = native_tls::Certificate::from_pem(include_bytes!("./riotgames.pem"))?;
    let tls = native_tls::TlsConnector::builder()
        .add_root_certificate(cert)
        .build()?;
    let connector = tokio_tungstenite::Connector::NativeTls(tls);
    {
        let headers = url.headers_mut();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(format!("Basic {}", auth_token).as_str())?,
        );
    }

    let (ws_stream, _response) =
        tokio_tungstenite::connect_async_tls_with_config(url, None, Some(connector)).await?;

    let (mut write, read) = ws_stream.split();
    write.send(Message::Text("[5, \"OnJsonApiEvent_lol-champ-select_v1_session\"]".into())).await?;

    // Reqwest stuff
    let reqwest_cert = Certificate::from_pem(include_bytes!("./riotgames.pem"))?;
    let mut headers = header::HeaderMap::new();
    let auth_header = header::HeaderValue::from_str(format!("Basic {}", auth_token).as_str()).unwrap();
    headers.insert("Authorization", auth_header);
    let client = reqwest::ClientBuilder::new()
                            .add_root_certificate(reqwest_cert)
                            .default_headers(headers)
                            .build()?;
    // Reqwest stuff
    
    let read_messages = read.for_each(|message| async {
        let data = message.unwrap().into_text().unwrap();
        if data != "" {
            let model: Value = serde_json::from_str(&data).unwrap();
            let inner_data = &model[2]["data"].to_string();
            let champ_select: Value = serde_json::from_str(&inner_data).unwrap();
            let champ_select_ternifolia_url = Url::parse("https://ternifolia.com/university/champselect").unwrap();
            client.post(champ_select_ternifolia_url)
                  .json(&champ_select)
                  .send()
                  .await.unwrap();

            let mut summ_id_transform: HashMap<String, String> = HashMap::new();
            summ_id_transform.insert("0".to_string(), "Bot".to_string()); // Bot's id is 0

            let mut my_team:Vec<TeamElement> = serde_json::from_str(&champ_select["myTeam"].to_string()).unwrap();
            let mut their_team:Vec<TeamElement> = serde_json::from_str(&champ_select["theirTeam"].to_string()).unwrap();
            my_team.append(&mut their_team);

            for player in my_team {
                if player.summoner_id == 0 {
                    continue;
                } 
                let summoner_name_url = Url::parse(&format!("https://127.0.0.1:{}/lol-summoner/v1/summoners/{}", &lockfile.port, &player.summoner_id)).unwrap();
                let res: serde_json::Value = client.get(summoner_name_url)
                    .send()
                    .await.unwrap()
                    .json()
                    .await.unwrap();

                summ_id_transform.insert(player.summoner_id.to_string(), res["displayName"].to_string());
            }

            let summ_names_ternifolia_url = Url::parse("https://ternifolia.com/university/summonernames").unwrap();
            client.post(summ_names_ternifolia_url)
                  .json(&summ_id_transform)
                  .send()
                  .await.unwrap();
            println!("Sending request to ternifolia at {}", Utc::now());
        }
    });

    read_messages.await;

    Ok(())
}

fn encode_token(remote_token: &str) -> String {
    let token = format!("riot:{}", remote_token);
    base64::encode(token)
}
