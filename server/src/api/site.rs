use super::user::Register;
use crate::{
  api::{
    check_slurs,
    check_slurs_opt,
    get_user_from_jwt,
    get_user_from_jwt_opt,
    is_admin,
    APIError,
    Perform,
  },
  apub::fetcher::search_by_apub_id,
  blocking,
  version,
  websocket::{
    server::{GetUsersOnline, SendAllMessage},
    UserOperation,
    WebsocketInfo,
  },
  DbPool,
  LemmyError,
};
use actix_web::client::Client;
use lemmy_db::{
  category::*,
  comment_view::*,
  community_view::*,
  diesel_option_overwrite,
  moderator::*,
  moderator_views::*,
  naive_now,
  post_view::*,
  site::*,
  site_view::*,
  user::*,
  user_view::*,
  Crud,
  SearchType,
  SortType,
};
use lemmy_utils::settings::Settings;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize)]
pub struct ListCategories {}

#[derive(Serialize, Deserialize)]
pub struct ListCategoriesResponse {
  categories: Vec<Category>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Search {
  q: String,
  type_: String,
  community_id: Option<i32>,
  sort: String,
  page: Option<i64>,
  limit: Option<i64>,
  auth: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResponse {
  pub type_: String,
  pub comments: Vec<CommentView>,
  pub posts: Vec<PostView>,
  pub communities: Vec<CommunityView>,
  pub users: Vec<UserView>,
}

#[derive(Serialize, Deserialize)]
pub struct GetModlog {
  mod_user_id: Option<i32>,
  community_id: Option<i32>,
  page: Option<i64>,
  limit: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct GetModlogResponse {
  removed_posts: Vec<ModRemovePostView>,
  locked_posts: Vec<ModLockPostView>,
  stickied_posts: Vec<ModStickyPostView>,
  removed_comments: Vec<ModRemoveCommentView>,
  removed_communities: Vec<ModRemoveCommunityView>,
  banned_from_community: Vec<ModBanFromCommunityView>,
  banned: Vec<ModBanView>,
  added_to_community: Vec<ModAddCommunityView>,
  added: Vec<ModAddView>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateSite {
  pub name: String,
  pub description: Option<String>,
  pub icon: Option<String>,
  pub banner: Option<String>,
  pub enable_downvotes: bool,
  pub open_registration: bool,
  pub enable_nsfw: bool,
  pub auth: String,
}

#[derive(Serialize, Deserialize)]
pub struct EditSite {
  name: String,
  description: Option<String>,
  icon: Option<String>,
  banner: Option<String>,
  enable_downvotes: bool,
  open_registration: bool,
  enable_nsfw: bool,
  auth: String,
}

#[derive(Serialize, Deserialize)]
pub struct GetSite {
  auth: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SiteResponse {
  site: SiteView,
}

#[derive(Serialize, Deserialize)]
pub struct GetSiteResponse {
  site: Option<SiteView>,
  admins: Vec<UserView>,
  banned: Vec<UserView>,
  pub online: usize,
  version: String,
  my_user: Option<User_>,
  federated_instances: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TransferSite {
  user_id: i32,
  auth: String,
}

#[derive(Serialize, Deserialize)]
pub struct GetSiteConfig {
  auth: String,
}

#[derive(Serialize, Deserialize)]
pub struct GetSiteConfigResponse {
  config_hjson: String,
}

#[derive(Serialize, Deserialize)]
pub struct SaveSiteConfig {
  config_hjson: String,
  auth: String,
}

#[async_trait::async_trait(?Send)]
impl Perform for ListCategories {
  type Response = ListCategoriesResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<ListCategoriesResponse, LemmyError> {
    let _data: &ListCategories = &self;

    let categories = blocking(pool, move |conn| Category::list_all(conn)).await??;

    // Return the jwt
    Ok(ListCategoriesResponse { categories })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for GetModlog {
  type Response = GetModlogResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<GetModlogResponse, LemmyError> {
    let data: &GetModlog = &self;

    let community_id = data.community_id;
    let mod_user_id = data.mod_user_id;
    let page = data.page;
    let limit = data.limit;
    let removed_posts = blocking(pool, move |conn| {
      ModRemovePostView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    let locked_posts = blocking(pool, move |conn| {
      ModLockPostView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    let stickied_posts = blocking(pool, move |conn| {
      ModStickyPostView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    let removed_comments = blocking(pool, move |conn| {
      ModRemoveCommentView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    let banned_from_community = blocking(pool, move |conn| {
      ModBanFromCommunityView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    let added_to_community = blocking(pool, move |conn| {
      ModAddCommunityView::list(conn, community_id, mod_user_id, page, limit)
    })
    .await??;

    // These arrays are only for the full modlog, when a community isn't given
    let (removed_communities, banned, added) = if data.community_id.is_none() {
      blocking(pool, move |conn| {
        Ok((
          ModRemoveCommunityView::list(conn, mod_user_id, page, limit)?,
          ModBanView::list(conn, mod_user_id, page, limit)?,
          ModAddView::list(conn, mod_user_id, page, limit)?,
        )) as Result<_, LemmyError>
      })
      .await??
    } else {
      (Vec::new(), Vec::new(), Vec::new())
    };

    // Return the jwt
    Ok(GetModlogResponse {
      removed_posts,
      locked_posts,
      stickied_posts,
      removed_comments,
      removed_communities,
      banned_from_community,
      banned,
      added_to_community,
      added,
    })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for CreateSite {
  type Response = SiteResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<SiteResponse, LemmyError> {
    let data: &CreateSite = &self;

    let user = get_user_from_jwt(&data.auth, pool).await?;

    check_slurs(&data.name)?;
    check_slurs_opt(&data.description)?;

    // Make sure user is an admin
    is_admin(pool, user.id).await?;

    let site_form = SiteForm {
      name: data.name.to_owned(),
      description: data.description.to_owned(),
      icon: Some(data.icon.to_owned()),
      banner: Some(data.banner.to_owned()),
      creator_id: user.id,
      enable_downvotes: data.enable_downvotes,
      open_registration: data.open_registration,
      enable_nsfw: data.enable_nsfw,
      updated: None,
    };

    let create_site = move |conn: &'_ _| Site::create(conn, &site_form);
    if blocking(pool, create_site).await?.is_err() {
      return Err(APIError::err("site_already_exists").into());
    }

    let site_view = blocking(pool, move |conn| SiteView::read(conn)).await??;

    Ok(SiteResponse { site: site_view })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for EditSite {
  type Response = SiteResponse;
  async fn perform(
    &self,
    pool: &DbPool,
    websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<SiteResponse, LemmyError> {
    let data: &EditSite = &self;
    let user = get_user_from_jwt(&data.auth, pool).await?;

    check_slurs(&data.name)?;
    check_slurs_opt(&data.description)?;

    // Make sure user is an admin
    is_admin(pool, user.id).await?;

    let found_site = blocking(pool, move |conn| Site::read(conn, 1)).await??;

    let icon = diesel_option_overwrite(&data.icon);
    let banner = diesel_option_overwrite(&data.banner);

    let site_form = SiteForm {
      name: data.name.to_owned(),
      description: data.description.to_owned(),
      icon,
      banner,
      creator_id: found_site.creator_id,
      updated: Some(naive_now()),
      enable_downvotes: data.enable_downvotes,
      open_registration: data.open_registration,
      enable_nsfw: data.enable_nsfw,
    };

    let update_site = move |conn: &'_ _| Site::update(conn, 1, &site_form);
    if blocking(pool, update_site).await?.is_err() {
      return Err(APIError::err("couldnt_update_site").into());
    }

    let site_view = blocking(pool, move |conn| SiteView::read(conn)).await??;

    let res = SiteResponse { site: site_view };

    if let Some(ws) = websocket_info {
      ws.chatserver.do_send(SendAllMessage {
        op: UserOperation::EditSite,
        response: res.clone(),
        my_id: ws.id,
      });
    }

    Ok(res)
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for GetSite {
  type Response = GetSiteResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    websocket_info: Option<WebsocketInfo>,
    client: Client,
  ) -> Result<GetSiteResponse, LemmyError> {
    let data: &GetSite = &self;

    // TODO refactor this a little
    let res = blocking(pool, move |conn| Site::read(conn, 1)).await?;
    let site_view = if res.is_ok() {
      Some(blocking(pool, move |conn| SiteView::read(conn)).await??)
    } else if let Some(setup) = Settings::get().setup.as_ref() {
      let register = Register {
        username: setup.admin_username.to_owned(),
        email: setup.admin_email.to_owned(),
        password: setup.admin_password.to_owned(),
        password_verify: setup.admin_password.to_owned(),
        admin: true,
        show_nsfw: true,
        captcha_uuid: None,
        captcha_answer: None,
      };
      let login_response = register
        .perform(pool, websocket_info.clone(), client.clone())
        .await?;
      info!("Admin {} created", setup.admin_username);

      let create_site = CreateSite {
        name: setup.site_name.to_owned(),
        description: None,
        icon: None,
        banner: None,
        enable_downvotes: true,
        open_registration: true,
        enable_nsfw: true,
        auth: login_response.jwt,
      };
      create_site
        .perform(pool, websocket_info.clone(), client.clone())
        .await?;
      info!("Site {} created", setup.site_name);
      Some(blocking(pool, move |conn| SiteView::read(conn)).await??)
    } else {
      None
    };

    let mut admins = blocking(pool, move |conn| UserView::admins(conn)).await??;

    // Make sure the site creator is the top admin
    if let Some(site_view) = site_view.to_owned() {
      let site_creator_id = site_view.creator_id;
      // TODO investigate why this is sometimes coming back null
      // Maybe user_.admin isn't being set to true?
      if let Some(creator_index) = admins.iter().position(|r| r.id == site_creator_id) {
        let creator_user = admins.remove(creator_index);
        admins.insert(0, creator_user);
      }
    }

    let banned = blocking(pool, move |conn| UserView::banned(conn)).await??;

    let online = if let Some(ws) = websocket_info {
      ws.chatserver.send(GetUsersOnline).await.unwrap_or(1)
    } else {
      0
    };

    let my_user = get_user_from_jwt_opt(&data.auth, pool).await?.map(|mut u| {
      u.password_encrypted = "".to_string();
      u.private_key = None;
      u.public_key = None;
      u
    });

    Ok(GetSiteResponse {
      site: site_view,
      admins,
      banned,
      online,
      version: version::VERSION.to_string(),
      my_user,
      federated_instances: Settings::get().get_allowed_instances(),
    })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for Search {
  type Response = SearchResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    client: Client,
  ) -> Result<SearchResponse, LemmyError> {
    let data: &Search = &self;

    dbg!(&data);

    match search_by_apub_id(&data.q, &client, pool).await {
      Ok(r) => return Ok(r),
      Err(e) => debug!("Failed to resolve search query as activitypub ID: {}", e),
    }

    let user = get_user_from_jwt_opt(&data.auth, pool).await?;
    let user_id = user.map(|u| u.id);

    let type_ = SearchType::from_str(&data.type_)?;

    let mut posts = Vec::new();
    let mut comments = Vec::new();
    let mut communities = Vec::new();
    let mut users = Vec::new();

    // TODO no clean / non-nsfw searching rn

    let q = data.q.to_owned();
    let page = data.page;
    let limit = data.limit;
    let sort = SortType::from_str(&data.sort)?;
    let community_id = data.community_id;
    match type_ {
      SearchType::Posts => {
        posts = blocking(pool, move |conn| {
          PostQueryBuilder::create(conn)
            .sort(&sort)
            .show_nsfw(true)
            .for_community_id(community_id)
            .search_term(q)
            .my_user_id(user_id)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
      SearchType::Comments => {
        comments = blocking(pool, move |conn| {
          CommentQueryBuilder::create(&conn)
            .sort(&sort)
            .search_term(q)
            .my_user_id(user_id)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
      SearchType::Communities => {
        communities = blocking(pool, move |conn| {
          CommunityQueryBuilder::create(conn)
            .sort(&sort)
            .search_term(q)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
      SearchType::Users => {
        users = blocking(pool, move |conn| {
          UserQueryBuilder::create(conn)
            .sort(&sort)
            .search_term(q)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
      SearchType::All => {
        posts = blocking(pool, move |conn| {
          PostQueryBuilder::create(conn)
            .sort(&sort)
            .show_nsfw(true)
            .for_community_id(community_id)
            .search_term(q)
            .my_user_id(user_id)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;

        let q = data.q.to_owned();
        let sort = SortType::from_str(&data.sort)?;

        comments = blocking(pool, move |conn| {
          CommentQueryBuilder::create(conn)
            .sort(&sort)
            .search_term(q)
            .my_user_id(user_id)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;

        let q = data.q.to_owned();
        let sort = SortType::from_str(&data.sort)?;

        communities = blocking(pool, move |conn| {
          CommunityQueryBuilder::create(conn)
            .sort(&sort)
            .search_term(q)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;

        let q = data.q.to_owned();
        let sort = SortType::from_str(&data.sort)?;

        users = blocking(pool, move |conn| {
          UserQueryBuilder::create(conn)
            .sort(&sort)
            .search_term(q)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
      SearchType::Url => {
        posts = blocking(pool, move |conn| {
          PostQueryBuilder::create(conn)
            .sort(&sort)
            .show_nsfw(true)
            .for_community_id(community_id)
            .url_search(q)
            .page(page)
            .limit(limit)
            .list()
        })
        .await??;
      }
    };

    // Return the jwt
    Ok(SearchResponse {
      type_: data.type_.to_owned(),
      comments,
      posts,
      communities,
      users,
    })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for TransferSite {
  type Response = GetSiteResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<GetSiteResponse, LemmyError> {
    let data: &TransferSite = &self;
    let mut user = get_user_from_jwt(&data.auth, pool).await?;

    // TODO add a User_::read_safe() for this.
    user.password_encrypted = "".to_string();
    user.private_key = None;
    user.public_key = None;

    let read_site = blocking(pool, move |conn| Site::read(conn, 1)).await??;

    // Make sure user is the creator
    if read_site.creator_id != user.id {
      return Err(APIError::err("not_an_admin").into());
    }

    let new_creator_id = data.user_id;
    let transfer_site = move |conn: &'_ _| Site::transfer(conn, new_creator_id);
    if blocking(pool, transfer_site).await?.is_err() {
      return Err(APIError::err("couldnt_update_site").into());
    };

    // Mod tables
    let form = ModAddForm {
      mod_user_id: user.id,
      other_user_id: data.user_id,
      removed: Some(false),
    };

    blocking(pool, move |conn| ModAdd::create(conn, &form)).await??;

    let site_view = blocking(pool, move |conn| SiteView::read(conn)).await??;

    let mut admins = blocking(pool, move |conn| UserView::admins(conn)).await??;
    let creator_index = admins
      .iter()
      .position(|r| r.id == site_view.creator_id)
      .unwrap();
    let creator_user = admins.remove(creator_index);
    admins.insert(0, creator_user);

    let banned = blocking(pool, move |conn| UserView::banned(conn)).await??;

    Ok(GetSiteResponse {
      site: Some(site_view),
      admins,
      banned,
      online: 0,
      version: version::VERSION.to_string(),
      my_user: Some(user),
      federated_instances: Settings::get().get_allowed_instances(),
    })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for GetSiteConfig {
  type Response = GetSiteConfigResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<GetSiteConfigResponse, LemmyError> {
    let data: &GetSiteConfig = &self;
    let user = get_user_from_jwt(&data.auth, pool).await?;

    // Only let admins read this
    is_admin(pool, user.id).await?;

    let config_hjson = Settings::read_config_file()?;

    Ok(GetSiteConfigResponse { config_hjson })
  }
}

#[async_trait::async_trait(?Send)]
impl Perform for SaveSiteConfig {
  type Response = GetSiteConfigResponse;

  async fn perform(
    &self,
    pool: &DbPool,
    _websocket_info: Option<WebsocketInfo>,
    _client: Client,
  ) -> Result<GetSiteConfigResponse, LemmyError> {
    let data: &SaveSiteConfig = &self;
    let user = get_user_from_jwt(&data.auth, pool).await?;

    // Only let admins read this
    let admins = blocking(pool, move |conn| UserView::admins(conn)).await??;
    let admin_ids: Vec<i32> = admins.into_iter().map(|m| m.id).collect();

    if !admin_ids.contains(&user.id) {
      return Err(APIError::err("not_an_admin").into());
    }

    // Make sure docker doesn't have :ro at the end of the volume, so its not a read-only filesystem
    let config_hjson = match Settings::save_config_file(&data.config_hjson) {
      Ok(config_hjson) => config_hjson,
      Err(_e) => return Err(APIError::err("couldnt_update_site").into()),
    };

    Ok(GetSiteConfigResponse { config_hjson })
  }
}
