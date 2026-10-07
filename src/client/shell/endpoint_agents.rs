use super::render::put_text;
use super::*;

pub(super) fn render_collapsed(
    buffer: &mut Buffer,
    area: Rect,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let rows = agent_rows(endpoints, active_endpoint_id, config);
    for (index, row) in rows.into_iter().take(area.height as usize).enumerate() {
        let rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
        if row.agent.focused {
            buffer.set_style(rect, Style::default().bg(config.palette.active_row_bg));
        }
        let initial = row.machine_label.chars().next().unwrap_or('?');
        put_text(
            buffer,
            rect.x,
            rect.y,
            rect.width,
            &format!(
                "{initial}{}",
                status_icon(row.agent.status, config.status_indicators)
            ),
            Style::default()
                .fg(if row.stale {
                    config.palette.overlay0
                } else {
                    status_color(row.agent.status, &config.palette)
                })
                .add_modifier(if row.stale {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                }),
        );
        hits.endpoint_agents
            .push((rect, row.endpoint_id, row.agent.pane_id));
    }
}

pub(super) fn render_expanded(
    buffer: &mut Buffer,
    area: Rect,
    agent_view_label: Option<&str>,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
) {
    if !super::agent_sidebar::render_agent_panel_header(
        buffer,
        area,
        agent_view_label,
        config,
        hits,
    ) {
        return;
    }
    let items = agent_panel_items(endpoints, active_endpoint_id, config);
    super::agent_sidebar::render_agent_list(
        buffer,
        area,
        &items,
        agent_view_label.map(|_| " no matching agents"),
        config,
        agent_scroll,
        hits,
        AgentPanelItem::lines,
        |buffer, rect, item, hits| match item {
            AgentPanelItem::Heading { label, stale } => {
                let base = Style::default()
                    .fg(if *stale {
                        config.palette.overlay0
                    } else {
                        config.palette.text
                    })
                    .add_modifier(Modifier::BOLD);
                let style = if *stale {
                    base
                } else {
                    config
                        .spaces
                        .machine_heading_style(label)
                        .map_or(base, |patch| crate::ui::apply_sidebar_token_style(base, patch))
                };
                put_text(buffer, rect.x, rect.y, rect.width, &format!(" ▾ {label}"), style);
            }
            AgentPanelItem::Agent(row) => {
                // Indent under machine headings, like workspaces in the machine list.
                let content = if config.agents.group_by_machine {
                    Rect::new(
                        rect.x.saturating_add(2),
                        rect.y,
                        rect.width.saturating_sub(2),
                        rect.height,
                    )
                } else {
                    rect
                };
                super::agent_sidebar::render_agent_row(buffer, content, &row.agent, config);
                if row.stale {
                    buffer.set_style(
                        rect,
                        Style::default()
                            .fg(config.palette.overlay0)
                            .add_modifier(Modifier::DIM),
                    );
                }
                hits.endpoint_agents
                    .push((rect, row.endpoint_id.clone(), row.agent.pane_id.clone()));
            }
        },
    );
}

impl ClientShellState {
    pub(super) fn reveal_endpoint_agent(
        &mut self,
        endpoint_id: &ClientEndpointId,
        pane_id: &str,
        body_height: u16,
    ) {
        if body_height == 0 {
            return;
        }
        let rows = agent_panel_items(&self.endpoints, &self.active_endpoint_id, &self.config);
        let Some(target) = rows.iter().position(|item| {
            matches!(item, AgentPanelItem::Agent(row)
                if &row.endpoint_id == endpoint_id && row.agent.pane_id == pane_id)
        }) else {
            return;
        };
        let heights = rows
            .iter()
            .map(|item| item.lines().max(1).min(u16::MAX as usize) as u16)
            .collect::<Vec<_>>();
        let mut gaps = vec![self.config.agents.row_gap; rows.len()];
        if let Some(last) = gaps.last_mut() {
            *last = 0;
        }
        self.agent_scroll = super::scroll::list_scroll_start_to_reveal(
            &heights,
            &gaps,
            body_height,
            self.agent_scroll,
            target,
        );
    }
}

enum AgentPanelItem {
    /// Machine heading (only with `ui.sidebar.agents.group_by_machine`).
    Heading { label: String, stale: bool },
    Agent(EndpointAgentRow),
}

impl AgentPanelItem {
    fn lines(&self) -> usize {
        match self {
            Self::Heading { .. } => 1,
            Self::Agent(row) => row.agent.rows.len(),
        }
    }
}

/// Agent rows for the expanded panel. Grouped by machine: rows keep their order within a
/// machine, machines follow the sidebar order, and each machine gets a heading row.
fn agent_panel_items(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<AgentPanelItem> {
    let mut rows = agent_rows(endpoints, active_endpoint_id, config);
    if !config.agents.group_by_machine {
        return rows.into_iter().map(AgentPanelItem::Agent).collect();
    }
    let position = |id: &ClientEndpointId| {
        endpoints
            .iter()
            .position(|endpoint| &endpoint.endpoint_id == id)
            .unwrap_or(usize::MAX)
    };
    rows.sort_by_cached_key(|row| position(&row.endpoint_id));
    let mut items = Vec::with_capacity(rows.len() + endpoints.len());
    let mut current: Option<ClientEndpointId> = None;
    for row in rows {
        if current.as_ref() != Some(&row.endpoint_id) {
            current = Some(row.endpoint_id.clone());
            items.push(AgentPanelItem::Heading {
                label: row.machine_label.clone(),
                stale: row.stale,
            });
        }
        items.push(AgentPanelItem::Agent(row));
    }
    items
}

fn apply_machine_color(
    agent: &mut super::agent_sidebar::AgentRow,
    config: &ClientShellConfig,
    machine: &str,
) {
    let Some(fg) = config
        .spaces
        .machine_heading_style(machine)
        .and_then(|style| style.fg)
    else {
        return;
    };
    for token in agent.rows.iter_mut().flatten() {
        let status = matches!(
            token.kind,
            crate::ui::ResolvedTokenKind::StateIcon | crate::ui::ResolvedTokenKind::StateText(_)
        );
        if !status && token.style.fg.is_none() {
            token.style.fg = Some(fg);
        }
    }
}

struct EndpointAgentRow {
    endpoint_id: ClientEndpointId,
    machine_label: String,
    stale: bool,
    agent: super::agent_sidebar::AgentRow,
}

fn agent_rows(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<EndpointAgentRow> {
    let mut rendered_rows = endpoints
        .iter()
        .filter_map(|endpoint| {
            endpoint.snapshot.as_deref().map(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .filter_map(|agent| {
                        super::agent_sidebar::agent_row(
                            snapshot,
                            &agent.pane_id,
                            config,
                            Some(&endpoint.label),
                        )
                    })
                    .map(|mut agent| {
                        if config.agents.machine_color {
                            apply_machine_color(&mut agent, config, &endpoint.label);
                        }
                        agent
                    })
                    .map(|agent| ((endpoint.endpoint_id.clone(), agent.pane_id.clone()), agent))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect::<HashMap<_, _>>();

    super::aggregate_navigation::aggregate_agent_rows(
        endpoints,
        active_endpoint_id,
        config.agent_panel_sort,
    )
    .into_iter()
    .filter_map(|row| {
        let key = (row.endpoint.endpoint_id.clone(), row.agent.pane_id.clone());
        let mut agent = rendered_rows.remove(&key)?;
        agent.focused &= row.endpoint.endpoint_id == active_endpoint_id;
        Some(EndpointAgentRow {
            endpoint_id: row.endpoint.endpoint_id.clone(),
            machine_label: row.endpoint.label.to_owned(),
            stale: row.endpoint.stale(),
            agent,
        })
    })
    .collect()
}
