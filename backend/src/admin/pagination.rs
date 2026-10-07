use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    CreatedAt,
    DisplayName,
    Email,
    Providers,
    NoticeCount,
    User,
    Ip,
    UserAgent,
    ExpiresAt,
    Text,
    Severity,
}

impl SortField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreatedAt => "createdAt",
            Self::DisplayName => "displayName",
            Self::Email => "email",
            Self::Providers => "providers",
            Self::NoticeCount => "noticeCount",
            Self::User => "user",
            Self::Ip => "ip",
            Self::UserAgent => "userAgent",
            Self::ExpiresAt => "expiresAt",
            Self::Text => "text",
            Self::Severity => "severity",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u16>,
    pub sort: Option<SortField>,
    pub descending: Option<bool>,
}

pub struct PageRequest {
    pub offset: i64,
    pub sort: SortField,
    pub descending: bool,
    pub search: String,
    pub limit: usize,
}

impl PageQuery {
    pub fn validate(self) -> Result<PageRequest, &'static str> {
        let limit = self.limit.unwrap_or(25);
        if !(1..=100).contains(&limit) {
            return Err("limit must be between 1 and 100");
        }
        let search = self.search.unwrap_or_default();
        if search.len() > 500 {
            return Err("search must be at most 500 bytes");
        }
        let offset = if let Some(cursor) = self.cursor {
            if cursor.len() > 32 {
                return Err("Invalid cursor");
            }
            let decoded = URL_SAFE_NO_PAD
                .decode(cursor)
                .map_err(|_| "Invalid cursor")?;
            let decoded = String::from_utf8(decoded).map_err(|_| "Invalid cursor")?;
            let offset = decoded.parse::<i64>().map_err(|_| "Invalid cursor")?;
            if !(0..=i64::MAX - 101).contains(&offset) {
                return Err("Invalid cursor");
            }
            offset
        } else {
            0
        };
        let search = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        Ok(PageRequest {
            offset,
            sort: self.sort.unwrap_or(SortField::CreatedAt),
            descending: self.descending.unwrap_or(true),
            search: format!("%{search}%"),
            limit: usize::from(limit),
        })
    }
}

impl PageRequest {
    pub fn validate_sort(&self, fields: &[&str]) -> Result<(), &'static str> {
        if fields.contains(&self.sort.as_str()) {
            Ok(())
        } else {
            Err("Invalid sort field")
        }
    }

    pub fn fetch_limit(&self) -> i64 {
        i64::try_from(self.limit + 1).expect("page limit is at most 100")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn new(mut items: Vec<T>, request: &PageRequest) -> Self {
        let has_more = items.len() > request.limit;
        items.truncate(request.limit);
        let next_cursor = has_more.then(|| {
            URL_SAFE_NO_PAD.encode(
                (request.offset + i64::try_from(request.limit).expect("bounded page limit"))
                    .to_string(),
            )
        });
        Self { items, next_cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, PageQuery};

    #[test]
    fn rejects_invalid_page_boundaries() {
        for limit in [0, 101] {
            assert!(
                PageQuery {
                    limit: Some(limit),
                    ..PageQuery::default()
                }
                .validate()
                .is_err()
            );
        }
        for cursor in ["!", "LTE", "OTIyMzM3MjAzNjg1NDc3NTgwNw"] {
            assert!(
                PageQuery {
                    cursor: Some(cursor.to_owned()),
                    ..PageQuery::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn cursor_advances_without_exposing_the_lookahead_row() {
        let first = PageQuery {
            limit: Some(2),
            ..PageQuery::default()
        }
        .validate()
        .expect("valid page");
        let page = Page::new(vec![1, 2, 3], &first);
        assert_eq!(page.items, vec![1, 2]);
        let next = PageQuery {
            cursor: page.next_cursor,
            limit: Some(2),
            ..PageQuery::default()
        }
        .validate()
        .expect("valid cursor");
        assert_eq!(next.offset, 2);
        let final_page = Page::new(vec![3], &next);
        assert_eq!(final_page.items, vec![3]);
        assert!(final_page.next_cursor.is_none());
    }

    #[test]
    fn search_treats_sql_wildcards_as_literal_text() {
        let page = PageQuery {
            search: Some(r"50%_\".to_owned()),
            ..PageQuery::default()
        }
        .validate()
        .expect("valid search");
        assert_eq!(page.search, r"%50\%\_\\%");
    }
}
