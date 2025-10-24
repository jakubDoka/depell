#![feature(iter_collect_into, macro_metavar_expr)]
use {
    argon2::{password_hash::SaltString, PasswordVerifier},
    axum::{
        body::Bytes,
        extract::{DefaultBodyLimit, Path},
        http::{header::COOKIE, request::Parts, StatusCode},
        response::{AppendHeaders, Html},
    },
    const_format::formatcp,
    core::fmt,
    htmlm::{html, write_html},
    rand_core::OsRng,
    serde::{Deserialize, Serialize},
    std::{
        collections::{HashMap, HashSet},
        fmt::{Display, Write},
        net::Ipv4Addr,
    },
};

const MAX_NAME_LENGTH: usize = 32;
const MAX_POSTNAME_LENGTH: usize = 64;
const MAX_CODE_LENGTH: usize = 1024 * 4;
const SESSION_DURATION_SECS: u64 = 60 * 60;
const MAX_FEED_SIZE: usize = 8 * 1024;

type Redirect<const COUNT: usize = 1> = AppendHeaders<[(&'static str, String); COUNT]>;

macro_rules! static_asset {
    ($mime:literal, $body:literal) => {
        get(|| async {
            axum::http::Response::builder()
                .header("content-type", $mime)
                .header("content-encoding", "gzip")
                .body(axum::body::Body::from(Bytes::from_static(include_bytes!(
                    concat!($body, ".gz")
                ))))
                .unwrap()
        })
    };
}

async fn amain() {
    use axum::routing::{delete, get, post};

    let debug = cfg!(debug_assertions);

    log::set_logger(&Logger).unwrap();
    log::set_max_level(if debug {
        log::LevelFilter::Warn
    } else {
        log::LevelFilter::Error
    });

    db::init();

    let router = axum::Router::new()
        .route(
            "/index.css",
            static_asset!("text/css", "../zig-out/static/index.css"),
        )
        .route(
            "/index.js",
            static_asset!("text/javascript", "../zig-out/static/index.js"),
        )
        .route(
            "/hbfmt.wasm",
            static_asset!("application/wasm", "../zig-out/bin/hbfmt.wasm"),
        )
        .route(
            "/hbc.wasm",
            static_asset!("application/wasm", "../zig-out/bin/hbc.wasm"),
        )
        .route("/", get(Index::page))
        .route("/index-view", get(Index::get_with_blog))
        .route("/blogs/index-view", get(Index::get))
        .route("/blogs/developing-hblang", get(DevelopingHblang::page))
        .route("/blogs/developing-hblang-view", get(DevelopingHblang::get))
        .route("/blogs/hblang-report-1", get(HblangReport1::page))
        .route("/blogs/hblang-report-1-view", get(HblangReport1::get))
        .route("/feed", get(Feed::page))
        .route("/feed-view", get(Feed::get))
        .route("/feed-more", post(Feed::more))
        .route("/profile", get(Profile::page))
        .route("/profile-view", get(Profile::get))
        .route("/profile/:name", get(Profile::get_other_page))
        .route("/profile/password", post(PasswordChange::post))
        .route("/profile-view/:name", get(Profile::get_other))
        .route("/post", get(Post::page))
        .route("/post-view", get(Post::get))
        .route("/post", post(Post::post))
        .route("/post/run", post(Post::run))
        .route("/code", post(fetch_code))
        .route("/login", get(Login::page))
        .route("/login-view", get(Login::get))
        .route("/login", post(Login::post))
        .route("/login", delete(Login::delete))
        .route("/signup", get(Signup::page))
        .route("/signup-view", get(Signup::get))
        .route("/signup", post(Signup::post))
        .route(
            "/hot-reload",
            get({
                let id = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                move || async move { id.to_string() }
            }),
        )
        .layer(DefaultBodyLimit::max(16 * 1024));

    #[cfg(feature = "tls")]
    {
        let addr = (
            Ipv4Addr::UNSPECIFIED,
            std::env::var("DEPELL_PORT")
                .unwrap()
                .parse::<u16>()
                .unwrap(),
        );
        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            std::env::var("DEPELL_CERT_PATH").unwrap(),
            std::env::var("DEPELL_KEY_PATH").unwrap(),
        )
        .await
        .unwrap();

        axum_server::bind_rustls(addr.into(), config)
            .serve(router.into_make_service())
            .await
            .unwrap();
    }
    #[cfg(not(feature = "tls"))]
    {
        let addr = (Ipv4Addr::UNSPECIFIED, 8080);
        let socket = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(socket, router).await.unwrap();
    }
}

async fn fetch_code(
    axum::Json(paths): axum::Json<Vec<String>>,
) -> axum::Json<HashMap<String, String>> {
    let mut deps = HashMap::<String, String>::new();
    db::with(|db| {
        for path in &paths {
            let Some((author, name)) = path.split_once('/') else {
                continue;
            };
            db.fetch_deps
                .query_map((name, author), |r| {
                    Ok((
                        r.get::<_, String>(1)? + "/" + r.get_ref(0)?.as_str()?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .log("fetch deps query")
                .into_iter()
                .flatten()
                .filter_map(|r| r.log("deps row"))
                .collect_into(&mut deps);
        }
    });
    axum::Json(deps)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Feed {
    Before { before_timestamp: u64 },
}

#[derive(Deserialize)]
struct Before {
    before_timestamp: u64,
}

impl Feed {
    async fn more(session: Session, axum::Form(data): axum::Form<Before>) -> Html<String> {
        Self::Before {
            before_timestamp: data.before_timestamp,
        }
        .render(&session)
    }
}

impl Default for Feed {
    fn default() -> Self {
        Self::Before {
            before_timestamp: now() + 3600,
        }
    }
}

impl Page for Feed {
    fn render_to_buf(self, _: &Session, buf: &mut String) {
        db::with(|db| {
            let cursor = match self {
                Feed::Before { before_timestamp } => db
                    .get_pots_before
                    .query_map((before_timestamp,), Post::from_row)
                    .log("fetch before posts query")
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.log("fetch before posts row")),
            };

            let base_len = buf.len();
            let mut last_timestamp = None;
            for post in cursor {
                write!(buf, "{}", post).unwrap();
                if buf.len() - base_len > MAX_FEED_SIZE {
                    last_timestamp = Some(post.timestamp);
                    break;
                }
            }

            write_html!((*buf)
                if let Some(last_timestamp) = last_timestamp {
                    <div "hx-post"="/feed-more"
                        "hx-trigger"="intersect once"
                        "hx-swap"="outerHTML"
                        "hx-vals"={format_args!("{{\"before_timestamp\":{last_timestamp}}}")}
                    >"there might be more"</div>
                } else {
                    "no more stuff"
                }
            );
        });
    }
}

macro_rules! decl_static_pages {
    ($(
        #[derive(PublicPage)]
        #[page(static = $file:literal)]
        struct $name:ident$(($($extra:ident)*))?;
    )*) => {
        const ALL_STATIC_PAGES: [&str; ${count($file)}] = [$($file),*];

        $(
            #[derive(Default)]
            struct $name;

            impl PublicPage for $name {
                fn render_to_buf(self, buf: &mut String) {
                    buf.push_str(include_str!(concat!("../zig-out/static/", $file, ".html")));

                    $($($extra::default().render_to_buf(buf);)*)?
                }

                async fn page(session: Option<Session>) -> Html<String> {
                    base(|s| blog_base(s, |s| Self::default().render_to_buf(s)), session.as_ref())
                }
            }
        )*
    };
}

decl_static_pages! {
    #[derive(PublicPage)]
    #[page(static = "welcome")]
    struct Index(PostPreview);
    #[derive(PublicPage)]
    #[page(static = "developing-hblang")]
    struct DevelopingHblang;
    #[derive(PublicPage)]
    #[page(static = "hblang-report-1")]
    struct HblangReport1;
}

#[derive(Default)]
pub struct PostPreview;

impl PublicPage for PostPreview {
    fn render_to_buf(self, buf: &mut String) {
        Post {
            name: "playground".to_string(),
            code: r#"
print := fn(s: []u8): void @import("log")

main := fn(): uint {
    print("Hello, Depell!\n")
    return 42
}
"#
            .to_string(),

            uses_wasm: true,
            preview_mode: true,

            ..Default::default()
        }
        .render_to_buf(
            &Session {
                name: "anon".to_string(),
                id: [0; 32],
            },
            buf,
        );
    }
}

impl Index {
    async fn get_with_blog() -> Html<String> {
        let mut buf = String::new();
        blog_base(&mut buf, |s| Index.render_to_buf(s));
        Html(buf)
    }
}

#[derive(Deserialize, Default)]
struct Post {
    author: String,
    name: String,
    #[serde(default)]
    uses_wasm: bool,
    #[serde(skip)]
    timestamp: u64,
    #[serde(skip)]
    imports: usize,
    #[serde(skip)]
    runs: usize,
    #[serde(skip)]
    dependencies: usize,
    code: String,
    #[serde(skip)]
    error: Option<&'static str>,
    #[serde(skip)]
    preview_mode: bool,
}

impl Page for Post {
    fn render_to_buf(self, session: &Session, buf: &mut String) {
        let Self {
            name,
            code,
            error,
            uses_wasm,
            preview_mode,
            ..
        } = self;
        write_html! { (buf)
            <form id="postForm" "hx-post"="/post" "hx-swap"="outerHTML">
                if let Some(e) = error { <div class="error">e</div> }
                <input name="author" type="text" value={session.name} hidden>
                <div class="name-and-wasm-toggle">
                    <div class="toggle">
                        if uses_wasm {
                            <input type="checkbox" id="use-wasm-check" name="uses_wasm"
                                value=true checked hidden>
                        } else {
                            <input type="checkbox" id="use-wasm-check" name="uses_wasm"
                                value=true hidden>
                        }
                        <label for="use-wasm-check">"use wasm"</label>
                    </div>
                    if !preview_mode {
                        <input name={if preview_mode {"preview-name"} else {"name"}}
                            type="text" placeholder="name" value=name
                            required maxlength=MAX_POSTNAME_LENGTH>
                    } else {
                        <div class="placeholder">"preview"</div>
                    }
                </div>
                <div id="code-editor">
                    <textarea spellcheck=false id="code-edit" name={
                        if preview_mode {"code-preview"} else {"code"}}
                        placeholder="code" rows=1
                        required>code</textarea>
                    <pre id="highlighter"></pre>
                    <span id="code-size">MAX_CODE_LENGTH</span>
                </div>
                if !preview_mode {
                    <input type="submit" value="submit">
                }
                <pre id="compiler-output"></pre>
            </form>

            if !preview_mode {
                <div id="dep-list">
                    <input placeholder="search imported deps.." oninput="filterCodeDeps(this, event)">
                    <section id="deps">
                        "results show here..."
                    </section>
                </div>
            }

            if !preview_mode {
                <div>
                    !{include_str!("../zig-out/static/post.html")}
                </div>
            }
        }
    }
}

#[derive(Deserialize)]
struct Run {
    author: String,
    name: String,
}

impl Post {
    pub fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Post {
            author: r.get(0)?,
            name: r.get(1)?,
            uses_wasm: r.get(2)?,
            timestamp: r.get(3)?,
            code: r.get(4)?,
            imports: r.get(5)?,
            runs: r.get(6)?,
            ..Default::default()
        })
    }

    async fn run(
        session: Session,
        axum::extract::Query(run): axum::extract::Query<Run>,
    ) -> StatusCode {
        match db::with(|qes| qes.creata_run.insert((run.name, run.author, session.name))) {
            Ok(_) => StatusCode::OK,
            Err(e) => {
                log::error!("creating run record failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    async fn post(
        session: Session,
        axum::Form(mut data): axum::Form<Self>,
    ) -> Result<Redirect, Html<String>> {
        if data.name.len() > MAX_POSTNAME_LENGTH {
            data.error = Some(formatcp!(
                "name too long, max length is {MAX_POSTNAME_LENGTH}"
            ));
            return Err(data.render(&session));
        }

        if data.code.len() > MAX_CODE_LENGTH {
            data.error = Some(formatcp!("code too long, max length is {MAX_CODE_LENGTH}"));
            return Err(data.render(&session));
        }

        db::with(|db| {
            if let Err(e) = db.create_post.insert((
                &data.name,
                &session.name,
                &data.uses_wasm,
                now(),
                &data.code,
            )) {
                if let rusqlite::Error::SqliteFailure(e, _) = e {
                    if e.code == rusqlite::ErrorCode::ConstraintViolation {
                        data.error = Some("this name is already used");
                    }
                }
                data.error = data.error.or_else(|| {
                    log::error!("create post error: {e}");
                    Some("internal server error")
                });
            }

            for (author, name) in data
                .code
                .match_indices("@use(\"")
                .filter_map(|(idx, pat)| {
                    data.code[idx + pat.len()..][..(data.code.len() - (idx + pat.len())).min(512)]
                        .split_once('"')
                        .and_then(|(v, _)| v.split_once('/'))
                })
                .collect::<HashSet<_>>()
            {
                if db
                    .create_import
                    .insert((author, name, &session.name, &data.name))
                    .log("create import query")
                    .is_none()
                {
                    data.error = Some("internal server error");
                    return;
                };
            }
        });

        if data.error.is_some() {
            Err(data.render(&session))
        } else {
            Ok(redirect(format!("/profile/{}", session.name)))
        }
    }
}

impl fmt::Display for Post {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            author,
            name,
            timestamp,
            imports,
            runs,
            dependencies,
            code,
            uses_wasm,
            ..
        } = self;
        write_html! { f <div class="preview" "data-author"=author "data-name"=name>
            <div class="info">
                <span>
                    <a "hx-get"={format_args!("/profile-view/{author}")} href="" "hx-target"="main"
                        "hx-push-url"={format_args!("/profile/{author}")}
                        "hx-swam"="innerHTML">author</a>
                    "/"
                    name
                </span>
                <span apply="timestamp">timestamp</span>
                <span class="target">{if *uses_wasm {"wasm"} else {"hbvm"}}</span>
                for (name, count) in [include_str!("icons/download.svg"), include_str!("icons/run.svg"),  "deps"]
                    .iter()
                    .zip([imports, runs, dependencies])
                    .filter(|(_, &c)| c != 0)
                {
                    <div class="stat">!name count</div>
                }
            </div>
            <div class="code">
                <nav>
                    <button onmousedown="runPost(this)">!{include_str!("icons/run.svg")}</button>
                </nav>
                <pre apply="fmt">code</pre>
            </div>
            <pre hidden id="compiler-output"></pre>
            if *timestamp == 0 {
                <button "hx-get"="/post" "hx-swap"="outerHTML"
                    "hx-target"="[preview]">"edit"</button>
            }
        </div> }
        Ok(())
    }
}

#[derive(Deserialize, Default)]
struct PasswordChange {
    old_password: String,
    new_password: String,
    #[serde(skip)]
    error: Option<&'static str>,
}

impl PasswordChange {
    async fn post(
        session: Session,
        axum::Form(mut change): axum::Form<PasswordChange>,
    ) -> Html<String> {
        db::with(|que| {
            match que
                .authenticate
                .query_row((&session.name,), |r| r.get::<_, String>(1))
            {
                Ok(hash) if verify_password(&hash, &change.old_password).is_err() => {
                    change.error = Some("invalid credentials");
                }
                Ok(_) => {
                    let new_hashed = hash_password(&change.new_password);
                    match que
                        .change_passowrd
                        .execute((new_hashed, &session.name))
                        .log("execute update")
                    {
                        None => change.error = Some("intenal server error"),
                        Some(0) => change.error = Some("password is incorrect"),
                        Some(_) => {}
                    }
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    change.error = Some("invalid credentials");
                }
                Err(e) => {
                    log::error!("login queri failed: {e}");
                    change.error = Some("internal server error");
                }
            }
        });

        if change.error.is_some() {
            change.render(&session)
        } else {
            PasswordChange::default().render(&session)
        }
    }
}

impl Page for PasswordChange {
    fn render_to_buf(self, _: &Session, buf: &mut String) {
        let Self {
            old_password,
            new_password,
            error,
        } = self;
        write_html! { (buf)
            <form "hx-post"="/profile/password" "hx-swap"="outerHTML">
                if let Some(e) = error { <div class="error">e</div> }
                <input name="old_password" type="password" autocomplete="old-password"
                    placeholder="old password" value=old_password>
                <input name="new_password" type="password" autocomplete="new-password" placeholder="new password"
                    value=new_password>
                <input type="submit" value="submit">
            </form>
        }
    }
}

#[derive(Default)]
struct Profile {
    other: Option<String>,
}

impl Profile {
    async fn get_other(session: Session, Path(name): Path<String>) -> Html<String> {
        Profile { other: Some(name) }.render(&session)
    }

    async fn get_other_page(session: Session, Path(name): Path<String>) -> Html<String> {
        base(
            |b| Profile { other: Some(name) }.render_to_buf(&session, b),
            Some(&session),
        )
    }
}

impl Page for Profile {
    fn render_to_buf(self, session: &Session, buf: &mut String) {
        db::with(|db| {
            let name = self.other.as_ref().unwrap_or(&session.name);
            let iter = db
                .get_user_posts
                .query_map((name,), Post::from_row)
                .log("get user posts query")
                .into_iter()
                .flatten()
                .filter_map(|p| p.log("user post row"));
            write_html! { (*buf)
                if name == &session.name {
                    |b|{PasswordChange::default().render_to_buf(session, b)}
                }

                for post in iter {
                    !{post}
                } else {
                    "no posts"
                }
            }
        })
    }
}

fn hash_password(password: &str) -> String {
    use argon2::PasswordHasher;
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string()
}

fn verify_password(hash: &str, password: &str) -> Result<(), argon2::password_hash::Error> {
    argon2::Argon2::default()
        .verify_password(password.as_bytes(), &argon2::PasswordHash::new(hash)?)
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct Login {
    name: String,
    password: String,
    #[serde(skip)]
    error: Option<&'static str>,
}

impl PublicPage for Login {
    fn render_to_buf(self, buf: &mut String) {
        let Self {
            name,
            password,
            error,
        } = self;
        write_html! { (buf)
            <form "hx-post"="/login" "hx-swap"="outerHTML">
                if let Some(e) = error { <div class="error">e</div> }
                <input name="name" type="text" autocomplete="name" placeholder="name" value=name
                    required maxlength=MAX_NAME_LENGTH>
                <input name="password" type="password" autocomplete="current-password" placeholder="password"
                    value=password>
                <input type="submit" value="submit">
            </form>
        }
    }
}

impl Login {
    async fn post(
        axum::Form(mut data): axum::Form<Self>,
    ) -> Result<AppendHeaders<[(&'static str, String); 2]>, Html<String>> {
        let mut id = [0u8; 32];
        db::with(|db| {
            match db
                .authenticate
                .query_row((&data.name,), |r| r.get::<_, String>(1))
            {
                Ok(hash) => {
                    if verify_password(&hash, &data.password).is_err() {
                        data.error = Some("invalid credentials");
                    } else {
                        getrandom::fill(&mut id).unwrap();
                        if db
                            .login
                            .insert((id, &data.name, now() + SESSION_DURATION_SECS))
                            .log("create session query")
                            .is_none()
                        {
                            data.error = Some("internal server error");
                        }
                    }
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    data.error = Some("invalid credentials");
                }
                Err(e) => {
                    log::error!("login queri failed: {e}");
                    data.error = Some("internal server error");
                }
            }
        });

        if data.error.is_some() {
            Err(data.render())
        } else {
            Ok(AppendHeaders([
                ("hx-location", "/feed".into()),
                (
                    "set-cookie",
                    format!(
                        "id={}; SameSite=Strict; Secure; Max-Age={SESSION_DURATION_SECS}",
                        to_hex(&id)
                    ),
                ),
            ]))
        }
    }

    async fn delete(session: Session) -> Redirect {
        _ = db::with(|q| q.logout.execute((session.id,)).log("delete session query"));
        redirect("/login")
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Signup {
    name: String,
    new_password: String,
    confirm_password: String,
    #[serde(default)]
    confirm_no_password: bool,
    #[serde(skip)]
    error: Option<&'static str>,
}

impl PublicPage for Signup {
    fn render_to_buf(self, buf: &mut String) {
        let Signup {
            name,
            new_password,
            confirm_password,
            confirm_no_password,
            error,
        } = self;
        let vals = if confirm_no_password {
            "{\"confirm_no_password\":true}"
        } else {
            "{}"
        };
        write_html! { (buf)
            <form "hx-post"="/signup" "hx-swap"="outerHTML" "hx-vals"=vals>
                if let Some(e) = error { <div class="error">e</div> }
                <input name="name" type="text" autocomplete="name" placeholder="name" value=name
                    maxlength=MAX_NAME_LENGTH required>
                <input name="new_password" type="password" autocomplete="new-password" placeholder="new password"
                    value=new_password>
                <input name="confirm_password" type="password" autocomplete="confirm-password"
                    placeholder="confirm password" value=confirm_password>
                <input type="submit" value="submit">
            </form>
        }
    }
}

impl Signup {
    async fn post(axum::Form(mut data): axum::Form<Self>) -> Result<Redirect, Html<String>> {
        if data.name.len() > MAX_NAME_LENGTH {
            data.error = Some(formatcp!("name too long, max length is {MAX_NAME_LENGTH}"));
            return Err(data.render());
        }

        if !data.confirm_no_password && data.new_password.is_empty() {
            data.confirm_no_password = true;
            data.error = Some("Are you sure you don't want to use a password? (then submit again)");
            return Err(data.render());
        }

        db::with(|db| {
            match db
                .register
                .insert((&data.name, hash_password(&data.new_password)))
            {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    data.error = Some("username already taken");
                }
                Err(e) => {
                    log::error!("create user query: {e}");
                    data.error = Some("internal server error");
                }
            };
        });

        if data.error.is_some() {
            Err(data.render())
        } else {
            Ok(redirect("/login"))
        }
    }
}

fn blog_base(s: &mut String, body: impl FnOnce(&mut String)) {
    let nav_button = |f: &mut String, name: &str| {
        write_html! {(f)
            <button "hx-push-url"={format_args!("/blogs/{name}")}
                "hx-get"={format_args!("/blogs/{name}-view")}
                "hx-target"="main#blog"
                "hx-swap"="innerHTML">name</button>
        }
    };

    write_html! {(*s)
        <nav><section>
            <button "hx-push-url"="/" "hx-get"="/blogs/index-view"
                "hx-target"="main#blog" "hx-swap"="innerHTML">"welcome"</button>
            for name in &ALL_STATIC_PAGES[1..] {
                |f|{nav_button(f, name)}
            }
        </section></nav>
        <section id="post-form"></section>
        <main id="blog">|f|{body(f)}</main>
    }
}

fn base(body: impl FnOnce(&mut String), session: Option<&Session>) -> Html<String> {
    let username = session.map(|s| &s.name);

    let nav_button = |f: &mut String, name: &str| {
        write_html! {(f)
            <button "hx-push-url"={format_args!("/{name}")}
                "hx-get"={format_args!("/{name}-view")}
                "hx-target"="main"
                "hx-swap"="innerHTML">name</button>
        }
    };

    Html(html! {
        "<!DOCTYPE html>"
        <html lang="en">
            <head>
                <meta name="charset" content="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="description" content="code dependency hell socila media hblang">
                <link rel="stylesheet" href="/index.css">
                <title>"depell"</title>
            </head>
            <body hidden>
                <nav>
                    <button "hx-push-url"="/" "hx-get"="/index-view" "hx-target"="main" "hx-swap"="innerHTML">"depell"</button>
                    <section>
                        if let Some(username) = username {
                            <button "hx-push-url"={format_args!("/profile/{username}")} "hx-get"="/profile-view" "hx-target"="main"
                                "hx-swap"="innerHTML">username</button>
                            |f|{nav_button(f, "feed"); nav_button(f, "post")}
                            <button "hx-delete"="/login">"logout"</button>
                        } else {
                            |f|{nav_button(f, "login"); nav_button(f, "signup")}
                        }
                    </section>
                </nav>
                <section id="post-form"></section>
                <main>|f|{body(f)}</main>
            </body>
            <script src="https://unpkg.com/htmx.org@2.0.3/dist/htmx.min.js" integrity="sha384-0895/pl2MU10Hqc6jd4RvrthNlDiE9U1tWmX7WRESftEDRosgxNsQG/Ze9YMRzHq" crossorigin="anonymous"></script>
            <script type="module" src="/index.js"></script>
        </html>
    })
}

struct Session {
    name: String,
    id: [u8; 32],
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for Session {
    /// If the extractor fails it'll use this "rejection" type. A rejection is
    /// a kind of error that can be converted into a response.
    type Rejection = Redirect;

    /// Perform the extraction.
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let err = redirect("/login");

        let value = parts
            .headers
            .get_all(COOKIE)
            .into_iter()
            .find_map(|c| c.to_str().ok()?.trim().strip_prefix("id="))
            .map(|c| c.split_once(';').unwrap_or((c, "")).0)
            .ok_or(err.clone())?;
        let mut id = [0u8; 32];
        parse_hex(value, &mut id).ok_or(err.clone())?;

        let (name, expiration) = db::with(|db| {
            db.get_session
                .query_row((id,), |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))
                .log("fetching session")
                .ok_or(err.clone())
        })?;

        if expiration < now() {
            return Err(err);
        }

        Ok(Self { name, id })
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn parse_hex(hex: &str, dst: &mut [u8]) -> Option<()> {
    fn hex_to_nibble(b: u8) -> Option<u8> {
        Some(match b {
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            b'0'..=b'9' => b - b'0',
            _ => return None,
        })
    }

    if hex.len() != dst.len() * 2 {
        return None;
    }

    for (d, p) in dst.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        *d = (hex_to_nibble(p[0])? << 4) | hex_to_nibble(p[1])?;
    }

    Some(())
}

fn to_hex(src: &[u8]) -> String {
    use std::fmt::Write;
    let mut buf = String::new();
    for &b in src {
        write!(buf, "{b:02x}").unwrap()
    }
    buf
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(amain());
}

mod db {
    use std::cell::RefCell;

    macro_rules! gen_queries {
        ($vis:vis struct $name:ident {
            $($qname:ident: $code:expr,)*
        }) => {
            $vis struct $name<'a> {
                $($vis $qname: rusqlite::Statement<'a>,)*
            }

            impl<'a> $name<'a> {
                fn new(db: &'a rusqlite::Connection) -> Self {
                    Self {
                        $($qname: db.prepare($code).unwrap(),)*
                    }
                }
            }
        };
    }

    gen_queries! {
        pub struct Queries {
            register: "INSERT INTO user (name, password_hash) VALUES(?, ?)",
            change_passowrd: "UPDATE user SET password_hash = ? WHERE name = ?",
            authenticate: "SELECT name, password_hash FROM user WHERE name = ?",
            login: "INSERT OR REPLACE INTO session (id, username, expiration) VALUES(?, ?, ?)",
            logout: "DELETE FROM session WHERE id = ?",
            get_session: "SELECT username, expiration FROM session WHERE id = ?",
            get_user_posts: "SELECT author, name, uses_wasm, timestamp, code, (
                WITH RECURSIVE roots(name, author, code) AS (
                    SELECT name, author, code FROM post WHERE name = outher.name AND author = outher.author
                    UNION
                    SELECT post.name, post.author, post.code FROM
                        post JOIN import ON post.name = import.from_name
                            AND post.author = import.from_author
                        JOIN roots ON import.to_name = roots.name
                            AND import.to_author = roots.author
                ) SELECT (count(*) - 1) FROM roots
            ) AS imports, (
                WITH RECURSIVE roots(name, author, code) AS (
                    SELECT name, author, code FROM post WHERE name = outher.name AND author = outher.author
                    UNION
                    SELECT post.name, post.author, post.code FROM post
                        JOIN import ON post.name = import.from_name
                            AND post.author = import.from_author
                        JOIN roots ON import.to_name = roots.name
                            AND import.to_author = roots.author
                ) SELECT count(*) FROM roots
                    JOIN run ON roots.name = run.code_name
                        AND roots.author = run.code_author
            ) AS runs FROM post as outher WHERE author = ? ORDER BY timestamp DESC",
            // TODO: we might want to cache the recursive queries
            get_pots_before: "SELECT author, name, uses_wasm, timestamp, code, (
                WITH RECURSIVE roots(name, author, code) AS (
                    SELECT name, author, code FROM post WHERE name = outher.name AND author = outher.author
                    UNION
                    SELECT post.name, post.author, post.code FROM
                        post JOIN import ON post.name = import.from_name
                            AND post.author = import.from_author
                        JOIN roots ON import.to_name = roots.name
                            AND import.to_author = roots.author
                ) SELECT (count(*) - 1) FROM roots
            ) AS imports, (
                WITH RECURSIVE roots(name, author, code) AS (
                    SELECT name, author, code FROM post WHERE name = outher.name AND author = outher.author
                    UNION
                    SELECT post.name, post.author, post.code FROM post
                        JOIN import ON post.name = import.from_name
                            AND post.author = import.from_author
                        JOIN roots ON import.to_name = roots.name
                            AND import.to_author = roots.author
                ) SELECT count(*) FROM roots
                    JOIN run ON roots.name = run.code_name
                        AND roots.author = run.code_author
            ) as runs FROM post AS outher WHERE timestamp < ?",
            create_post: "INSERT INTO post (name, author, uses_wasm, timestamp, code) VALUES(?, ?, ?, ?, ?)",
            fetch_deps: "
                WITH RECURSIVE roots(name, author, code) AS (
                    SELECT name, author, code FROM post WHERE name = ? AND author = ?
                    UNION
                    SELECT post.name, post.author, post.code FROM
                        post JOIN import ON post.name = import.to_name
                            AND post.author = import.to_author
                        JOIN roots ON import.from_name = roots.name
                            AND import.from_author = roots.author
                ) SELECT * FROM roots;
            ",
            create_import: "INSERT INTO import(to_author, to_name, from_author, from_name)
               VALUES(?, ?, ?, ?)",
            creata_run: "INSERT OR IGNORE INTO run(code_name, code_author, runner) VALUES(?, ?, ?)",
        }
    }

    struct Db {
        queries: Queries<'static>,
        _db: Box<rusqlite::Connection>,
    }

    impl Db {
        fn new() -> Self {
            let db = Box::new(rusqlite::Connection::open("db.sqlite").unwrap());
            Self {
                queries: Queries::new(unsafe {
                    std::mem::transmute::<&rusqlite::Connection, &rusqlite::Connection>(&db)
                }),
                _db: db,
            }
        }
    }

    pub fn with<T>(with: impl FnOnce(&mut Queries) -> T) -> T {
        thread_local! { static DB_CONN: RefCell<Db> = RefCell::new(Db::new()); }
        DB_CONN.with_borrow_mut(|q| with(&mut q.queries))
    }

    pub fn init() {
        const SCHEMA_VERSION: usize = 1;
        const MIGRATIONS: &[&str] = &[include_str!("migrations/1.sql")];

        let db = rusqlite::Connection::open("db.sqlite").unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();

        let schema_version = db
            .pragma_query_value(None, "user_version", |v| v.get::<_, usize>(0))
            .unwrap();

        if schema_version != SCHEMA_VERSION {
            db.pragma_update(None, "user_version", SCHEMA_VERSION)
                .unwrap();
            for &mig in &MIGRATIONS[schema_version..] {
                db.execute_batch(mig).expect(mig);
            }

            unreachable!("migration failed");
        }

        Queries::new(&db);
    }
}

fn redirect(to: impl Into<String>) -> Redirect {
    AppendHeaders([("hx-location", to.into())])
}

trait PublicPage: Default {
    fn render_to_buf(self, buf: &mut String);

    fn render(self) -> Html<String> {
        let mut str = String::new();
        self.render_to_buf(&mut str);
        Html(str)
    }

    async fn get() -> Html<String> {
        Self::default().render()
    }

    async fn page(session: Option<Session>) -> Html<String> {
        base(|s| Self::default().render_to_buf(s), session.as_ref())
    }
}

trait Page: Default {
    fn render_to_buf(self, session: &Session, buf: &mut String);

    fn render(self, session: &Session) -> Html<String> {
        let mut str = String::new();
        self.render_to_buf(session, &mut str);
        Html(str)
    }

    async fn get(session: Session) -> Html<String> {
        Self::default().render(&session)
    }

    async fn page(session: Option<Session>) -> Result<Html<String>, axum::response::Redirect> {
        match session {
            Some(session) => Ok(base(
                |f| Self::default().render_to_buf(&session, f),
                Some(&session),
            )),
            None => Err(axum::response::Redirect::permanent("/login")),
        }
    }
}

trait ResultExt<O, E> {
    fn log(self, prefix: impl Display) -> Option<O>;
}

impl<O, E: Display> ResultExt<O, E> for Result<O, E> {
    fn log(self, prefix: impl Display) -> Option<O> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                log::error!("{prefix}: {e}");
                None
            }
        }
    }
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "{} - {}",
                record.module_path().unwrap_or("=="),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}
