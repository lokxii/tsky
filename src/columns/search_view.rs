use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
};

use atrium_api::types::Object;
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Span,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::{profile_page::ProfilePage, Column},
    components::{
        actor::{ActorBasic, ActorBasicWidget},
        composer::{
            textarea::{Input, Key},
            vim::{InputMode, Vim},
        },
        list::{List, ListState},
        separation::Separation,
    },
};

struct SearchFeed {
    view: Vec<ActorBasic>,
    state: ListState,
}

impl SearchFeed {
    fn new(view: Vec<ActorBasic>) -> Self {
        Self { view, state: ListState::default() }
    }
}

enum SearchWorkerMsg {
    Search(String),
    Close,
}

enum Focus {
    SearchBar,
    Results,
}

pub struct SearchView {
    searchbar: Vim,
    kv: Arc<Mutex<HashMap<String, Vec<ActorBasic>>>>,
    feed: Option<SearchFeed>,
    focus: Focus,
    tx: Sender<SearchWorkerMsg>,
}

macro_rules! request_retry {
    ($retry:expr, $request:expr) => {{
        let mut count = 0;
        loop {
            let r = $request;
            match r {
                Ok(output) => break Some(output),
                Err(e) => {
                    count += 1;
                    if count == $retry {
                        log::error!("{}", e);
                        break None;
                    }
                }
            }
        }
    }};
}

impl SearchView {
    pub fn new(agent: BskyAgent) -> Self {
        let searchbar =
            Vim::new(|i| !matches!(i, Input { key: Key::Enter, .. }));
        let kv = Arc::new(Mutex::new(HashMap::new()));
        let feed = None;
        let (tx, rx) = mpsc::channel();

        let kv_ = Arc::clone(&kv);
        tokio::spawn(async move {
            let kv = kv_;
            loop {
                use atrium_api::app::bsky::actor::search_actors_typeahead::{
                    OutputData, ParametersData,
                };

                let Ok(msg) = rx.recv() else {
                    log::error!(
                        "Error receiving request message in search view worker"
                    );
                    return;
                };

                match msg {
                    SearchWorkerMsg::Search(s) => {
                        {
                            let kv = kv.lock().unwrap();
                            if kv.contains_key(&s) {
                                continue;
                            }
                        }
                        let Some(Object { data, .. }) = request_retry!(
                            3,
                            agent
                                .api
                                .app
                                .bsky
                                .actor
                                .search_actors_typeahead(
                                    ParametersData {
                                        limit: Some(8.try_into().unwrap()),
                                        q: Some(s.clone()),
                                        term: None,
                                    }
                                    .into(),
                                )
                                .await
                        ) else {
                            continue;
                        };
                        let OutputData { actors } = data;
                        let actors = actors
                            .iter()
                            .map(|Object { data, .. }| ActorBasic::from(data))
                            .collect::<Vec<_>>();
                        let mut kv = kv.lock().unwrap();
                        kv.insert(s, actors);
                    }
                    SearchWorkerMsg::Close => {
                        return;
                    }
                }
            }
        });

        Self { searchbar, kv, feed, focus: Focus::SearchBar, tx }
    }

    pub fn refresh(&mut self) {
        let s = self.searchbar.textarea.lines().join("").trim().to_string();
        let kv = self.kv.lock().unwrap();
        if let Some(a) = kv.get(&s) {
            if let Some(feed) = &self.feed {
                let mut state = feed.state.clone();
                let feed = SearchFeed::new(a.clone());
                if matches!(state.selected, Some(i) if i >= feed.view.len()) {
                    state = ListState::default();
                    state.selected = Some(feed.view.len() - 1);
                }
                self.feed = Some(feed);
                self.feed.as_mut().unwrap().state = state;
            } else {
                self.feed = Some(SearchFeed::new(a.clone()));
            }
        }
    }

    fn handle_pasting(&mut self, s: String) {
        if matches!(self.focus, Focus::SearchBar) {
            self.searchbar.textarea.insert_string(s);
        }
    }

    fn send_search_requet(&self) {
        let q = self.searchbar.textarea.lines().join("").trim().to_string();
        if q.is_empty() {
            return;
        }
        self.tx.send(SearchWorkerMsg::Search(q)).unwrap();
    }
}

impl Drop for SearchView {
    fn drop(&mut self) {
        self.tx.send(SearchWorkerMsg::Close).unwrap();
    }
}

impl EventReceiver for &mut SearchView {
    async fn handle_events(
        self,
        event: ratatui::crossterm::event::Event,
        agent: BskyAgent,
    ) -> crate::app::AppEvent {
        let key = match event.clone() {
            Event::Key(key) => key,
            Event::Paste(s) => {
                self.handle_pasting(s);
                self.send_search_requet();
                return AppEvent::None;
            }
            _ => return AppEvent::None,
        };
        if key.kind != event::KeyEventKind::Press {
            return AppEvent::None;
        }

        match self.focus {
            Focus::SearchBar => match event.clone().into() {
                Input { key: Key::Tab, .. } => {
                    self.focus = Focus::Results;
                    return AppEvent::None;
                }
                Input { key: Key::Backspace, .. }
                    if matches!(self.searchbar.mode, InputMode::Normal) =>
                {
                    return AppEvent::ColumnPopLayer;
                }
                _ => {
                    let r = self.searchbar.handle_events(event, agent).await;
                    self.send_search_requet();
                    return r;
                }
            },
            Focus::Results => match key.code {
                KeyCode::Tab => {
                    self.focus = Focus::SearchBar;
                    return AppEvent::None;
                }
                KeyCode::Backspace => {
                    return AppEvent::ColumnPopLayer;
                }

                KeyCode::Char('j') => {
                    let Some(feed) = self.feed.as_mut() else {
                        return AppEvent::None;
                    };
                    if feed.state.selected == None {
                        feed.state.selected = Some(0);
                    } else {
                        if feed.state.selected.unwrap() < feed.view.len() - 1 {
                            feed.state.next();
                        }
                    }
                    return AppEvent::None;
                }
                KeyCode::Char('k') => {
                    let Some(feed) = self.feed.as_mut() else {
                        return AppEvent::None;
                    };
                    feed.state.previous();
                    return AppEvent::None;
                }

                KeyCode::Enter => {
                    let Some(feed) = &self.feed else {
                        return AppEvent::None;
                    };
                    let Some(i) = feed.state.selected else {
                        return AppEvent::None;
                    };
                    let did = feed.view[i].did.clone();
                    let me = &agent.get_session().await.unwrap().did;
                    let profile = ProfilePage::from_did(did, me, agent);
                    return AppEvent::ColumnNewLayer(Column::ProfilePage(
                        profile,
                    ));
                }

                _ => return AppEvent::None,
            },
        }
    }
}

impl Widget for &mut SearchView {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let [_, searchbar_area, separation_area, results_area] =
            Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .areas(area);

        let title = match (&self.focus, &self.searchbar.mode) {
            (Focus::Results, _) => "Query",
            (_, InputMode::Normal) => "Query (Normal)",
            (_, InputMode::Insert) => "Query (Insert)",
            (_, InputMode::Visual) => "Query (View)",
        };
        self.searchbar.textarea.block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Color::DarkGray)
                .title(Span::styled(title, Color::Gray)),
        );
        self.searchbar.textarea.focused(matches!(self.focus, Focus::SearchBar));
        self.searchbar.textarea.render(searchbar_area, buf);

        Separation::default().padding(1).render(separation_area, buf);

        if let Some(feed) = self.feed.as_mut() {
            let items = feed.view.clone();
            List::new(items.len(), |context| {
                let style = if context.is_selected {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                };
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Color::DarkGray)
                    .style(style);
                let item =
                    ActorBasicWidget::new(&items[context.index]).block(block);

                (item, 3)
            })
            .render(results_area, buf, &mut feed.state);
        }
    }
}
