//! `/item` (ItemCommands) and `if items`/`if slots`: addressing the item
//! slots of the player, mobs, armor stands, item frames and containers.
use super::{Arg, Context, Game, R, Target, err};
use horizon_client::{
    command::{CommandError, args::ItemInput},
    items::ItemStack,
    text::Text,
};
use horizon_world::nbt::Tag;

/// One addressable slot.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::game) enum Slot {
    /// `inventory[0..36]`: hotbar then the main rows.
    Inv(usize),
    /// `armor[0..4]` (head, chest, legs, feet).
    Armor(usize),
    Offhand,
    Ender(usize),
    /// A mob's equipment by name (`mainhand`, `offhand`, `head`, ...).
    Mob(u32, &'static str),
    /// An armor stand's equipment (`ArmorStand::SLOTS` index).
    Stand(u32, usize),
    Frame(u32),
    Block([i32; 3], usize),
    /// A slot of another player (`for_player`): reached with their state in place.
    Guest(u32, Box<Slot>),
}
/// Whose slots a command addresses.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::game) enum Owner {
    Entity(Target),
    Block([i32; 3]),
}

/// The player's slot names and the inventory index each addresses.
fn player_slot_names() -> Vec<(String, usize)> {
    let mut v = Vec::new();
    for i in 0..36 {
        v.push((format!("container.{i}"), i));
    }
    for i in 0..9 {
        v.push((format!("hotbar.{i}"), i));
    }
    for i in 0..27 {
        v.push((format!("inventory.{i}"), 9 + i));
    }
    v
}
/// `name` against a group: `container.*` matches every `container.N`.
fn matches_name(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix(".*") {
        Some(prefix) => name.starts_with(&format!("{prefix}.")),
        None => pattern == name,
    }
}

impl Game<'_> {
    /// The block or the entities a `block <pos>` / `entity <name>` pair of
    /// ItemCommands or `execute if items` names: the first such pair, or with
    /// `source` the one after `from`.
    fn owner_of(&mut self, ctx: &Context, name: &str, source: bool) -> R<Vec<Owner>> {
        let kind = ctx
            .literals
            .iter()
            .filter(|l| matches!(l.as_str(), "entity" | "block"))
            .nth(usize::from(source))
            .cloned()
            .unwrap_or_default();
        let name = if source { "source" } else { name };
        if kind == "block" {
            let pos = if source { "source" } else { "pos" };
            return Ok(vec![Owner::Block(self.block_pos_arg(ctx, pos)?)]);
        }
        let targets = self.entities(ctx, name, false)?;
        Ok(targets.into_iter().map(Owner::Entity).collect())
    }
    /// ItemCommands.getContainer: the container block entity at `p`.
    fn container_at(&self, p: [i32; 3], key: &str) -> R<()> {
        match self.world.block_entities.get(&(p[0], p[1], p[2])) {
            Some(horizon_client::containers::BlockEntity::Chest(_)) => Ok(()),
            _ => Err(err(key, &[&p[0], &p[1], &p[2]])),
        }
    }
    /// The one slot a SlotArgument names on an owner (None when it has none).
    fn one_slot(&mut self, o: &Owner, name: &str) -> Option<Slot> {
        self.slots_named(o, name).into_iter().next()
    }
    /// The slots a name (or wildcard group) addresses on an owner.
    pub(in crate::game) fn slots_named(&mut self, owner: &Owner, name: &str) -> Vec<Slot> {
        if let Owner::Entity(Target::Guest(conn)) = *owner {
            let slots = self.for_player(Target::Guest(conn), |g| {
                g.slots_named(&Owner::Entity(Target::Player), name)
            });
            return slots
                .unwrap_or_default()
                .into_iter()
                .map(|s| Slot::Guest(conn, Box::new(s)))
                .collect();
        }
        let mut out = Vec::new();
        let mut add = |s: Slot| out.push(s);
        match owner {
            Owner::Block(p) => {
                let size = match self.world.block_entities.get(&(p[0], p[1], p[2])) {
                    Some(horizon_client::containers::BlockEntity::Chest(items)) => items.len(),
                    _ => 0,
                };
                for i in 0..size {
                    if matches_name(name, &format!("container.{i}")) {
                        add(Slot::Block(*p, i));
                    }
                }
            }
            Owner::Entity(Target::Player) => {
                for (n, i) in player_slot_names() {
                    if matches_name(name, &n) {
                        add(Slot::Inv(i));
                    }
                }
                for (i, n) in ["armor.head", "armor.chest", "armor.legs", "armor.feet"]
                    .into_iter()
                    .enumerate()
                {
                    if matches_name(name, n) {
                        add(Slot::Armor(i));
                    }
                }
                if matches_name(name, "weapon") || matches_name(name, "weapon.mainhand") {
                    add(Slot::Inv(self.selected));
                }
                if matches_name(name, "weapon.offhand") {
                    add(Slot::Offhand);
                }
                for i in 0..27 {
                    if matches_name(name, &format!("enderchest.{i}")) {
                        add(Slot::Ender(i));
                    }
                }
            }
            Owner::Entity(Target::Mob(id)) => {
                for (n, key) in [
                    ("weapon", "mainhand"),
                    ("weapon.mainhand", "mainhand"),
                    ("weapon.offhand", "offhand"),
                    ("armor.head", "head"),
                    ("armor.chest", "chest"),
                    ("armor.legs", "legs"),
                    ("armor.feet", "feet"),
                    ("armor.body", "body"),
                    ("saddle", "saddle"),
                ] {
                    if matches_name(name, n) {
                        add(Slot::Mob(*id, key));
                    }
                }
            }
            Owner::Entity(Target::Stand(id)) => {
                for (i, key) in horizon_client::entities::ArmorStand::SLOTS
                    .iter()
                    .enumerate()
                {
                    let aliases: &[&str] = match *key {
                        "mainhand" => &["weapon", "weapon.mainhand"],
                        "offhand" => &["weapon.offhand"],
                        "feet" => &["armor.feet"],
                        "legs" => &["armor.legs"],
                        "chest" => &["armor.chest"],
                        _ => &["armor.head"],
                    };
                    if aliases.iter().any(|a| matches_name(name, a)) {
                        add(Slot::Stand(*id, i));
                    }
                }
            }
            Owner::Entity(Target::Frame(id)) => {
                if matches_name(name, "container.0") || matches_name(name, "contents") {
                    add(Slot::Frame(*id));
                }
            }
            Owner::Entity(_) => {}
        }
        out
    }
    pub(in crate::game) fn slot_get(&mut self, s: &Slot) -> Option<ItemStack> {
        match s {
            Slot::Guest(conn, slot) => self
                .for_player(Target::Guest(*conn), |g| g.slot_get(slot))
                .flatten(),
            Slot::Inv(i) => self.inventory.get(*i).cloned().flatten(),
            Slot::Armor(i) => self.armor.get(*i).cloned().flatten(),
            Slot::Offhand => self.offhand.clone(),
            Slot::Ender(i) => self.ender_items.get(*i).cloned().flatten(),
            Slot::Mob(id, key) => {
                let m = self.mobs.iter().find(|m| m.id == *id)?;
                let item = match *key {
                    "mainhand" => &m.extra.main_hand,
                    "offhand" => &m.extra.off_hand,
                    "head" | "body" => &m.extra.head,
                    "chest" => &m.extra.chest,
                    "legs" => &m.extra.legs,
                    "feet" => &m.extra.feet,
                    _ => return None,
                };
                item.as_ref().map(|i| ItemStack {
                    item: i.clone(),
                    count: 1,
                    damage: 0,
                    extra: None,
                })
            }
            Slot::Stand(id, i) => self
                .armor_stands
                .iter()
                .find(|a| a.id == *id)?
                .equipment
                .get(*i)
                .cloned()
                .flatten(),
            Slot::Frame(id) => self.item_frames.iter().find(|f| f.id == *id)?.item.clone(),
            Slot::Block(p, i) => match self.world.block_entities.get(&(p[0], p[1], p[2])) {
                Some(horizon_client::containers::BlockEntity::Chest(items)) => {
                    items.get(*i).cloned().flatten()
                }
                _ => None,
            },
        }
    }
    /// Put a stack (or nothing) in a slot; false when the slot cannot take it.
    pub(in crate::game) fn slot_set(&mut self, s: &Slot, stack: Option<ItemStack>) -> bool {
        match s {
            Slot::Guest(conn, slot) => self
                .for_player(Target::Guest(*conn), |g| g.slot_set(slot, stack))
                .unwrap_or(false),
            Slot::Inv(i) => match self.inventory.get_mut(*i) {
                Some(slot) => {
                    *slot = stack;
                    true
                }
                None => false,
            },
            // LivingEntity's equipment SlotAccess: setItemSlot, so onEquipItem.
            Slot::Armor(i) => match self.armor.get_mut(*i) {
                Some(slot) => {
                    let old = std::mem::replace(slot, stack.clone());
                    let name = ["head", "chest", "legs", "feet"][*i];
                    self.player_on_equip(name, old.as_ref(), stack.as_ref());
                    self.apply_equipment();
                    true
                }
                None => false,
            },
            Slot::Offhand => {
                let old = std::mem::replace(&mut self.offhand, stack.clone());
                self.player_on_equip("offhand", old.as_ref(), stack.as_ref());
                true
            }
            Slot::Ender(i) => match self.ender_items.get_mut(*i) {
                Some(slot) => {
                    *slot = stack;
                    true
                }
                None => false,
            },
            Slot::Mob(id, key) => {
                let Some(m) = self.mobs.iter_mut().find(|m| m.id == *id) else {
                    return false;
                };
                let item = stack.as_ref().map(|s| s.item.clone());
                match *key {
                    "mainhand" => m.extra.set_main_hand_stack(stack),
                    "offhand" => m.extra.off_hand = item,
                    "head" | "body" => m.extra.head = item,
                    "chest" => m.extra.chest = item,
                    "legs" => m.extra.legs = item,
                    "feet" => m.extra.feet = item,
                    _ => return false,
                }
                true
            }
            Slot::Stand(id, i) => match self
                .armor_stands
                .iter_mut()
                .find(|a| a.id == *id)
                .and_then(|a| a.equipment.get_mut(*i))
            {
                Some(slot) => {
                    *slot = stack;
                    true
                }
                None => false,
            },
            Slot::Frame(id) => match self.item_frames.iter_mut().find(|f| f.id == *id) {
                Some(f) => {
                    f.item = stack;
                    true
                }
                None => false,
            },
            Slot::Block(p, i) => match self.world.block_entities.get_mut(&(p[0], p[1], p[2])) {
                Some(horizon_client::containers::BlockEntity::Chest(items)) => {
                    match items.get_mut(*i) {
                        Some(slot) => {
                            *slot = stack;
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            },
        }
    }
    fn owner_name(&self, o: &Owner) -> Text {
        match o {
            Owner::Entity(t) => self.target_name(*t),
            Owner::Block(p) => Text::literal(format!("{}, {}, {}", p[0], p[1], p[2])),
        }
    }
    fn stack_name(&self, s: &ItemStack) -> Text {
        self.stack_display_name(s)
    }
    fn item_from_input(&self, ctx: &Context) -> R<Option<ItemStack>> {
        let Some(Arg::Item(ItemInput { id, components, .. })) = ctx.get("item").cloned() else {
            return Ok(None);
        };
        let (damage, extra) = match components
            .as_deref()
            .map(horizon_client::item_data::command_components)
        {
            Some(Ok(tag)) => horizon_client::item_data::decode_components(Some(&tag)),
            Some(Err(e)) => return Err(CommandError::new(Text::literal(e))),
            None => (0, None),
        };
        let count = Self::int(ctx, "count").unwrap_or(1).max(1) as u32;
        let stack = ItemStack {
            item: id.as_str().into(),
            count,
            damage,
            extra,
        };
        // ItemInput.createItemStack: a count above the stack's max (its
        // max_stack_size component, else the item's) is refused.
        let max_stack = self.stack_max_size(&stack);
        if count > max_stack {
            return Err(err("arguments.item.overstacked", &[&id, &max_stack]));
        }
        Ok(Some(stack))
    }
    /// One item modifier (a typed object, a list or a datapack reference)
    /// applied to a stack; `None` when the stack ends up empty.
    fn apply_item_modifier(
        &self,
        m: &serde_json::Value,
        stack: Option<ItemStack>,
        rng: &mut horizon_core::random::LegacyRandom,
        this: Option<&horizon_client::advancements::EntityView>,
        origin: [f64; 3],
        depth: u32,
    ) -> Option<ItemStack> {
        use horizon_core::random::RandomSource;
        use serde_json::Value;
        if depth > 16 {
            return stack;
        }
        let int = |v: &Value, rng: &mut horizon_core::random::LegacyRandom| -> i32 {
            if let Some(n) = v.as_f64() {
                return n as i32;
            }
            let min = v["min"].as_f64().unwrap_or(0.) as i32;
            let max = v["max"].as_f64().unwrap_or(0.) as i32;
            match v["type"].as_str().unwrap_or("minecraft:constant") {
                "minecraft:uniform" | "uniform" => rng
                    .next_int_between_inclusive(min, max.max(min))
                    .unwrap_or(min),
                _ => v["value"].as_f64().unwrap_or(0.) as i32,
            }
        };
        match m {
            Value::Array(list) => list.iter().fold(stack, |s, f| {
                self.apply_item_modifier(f, s, rng, this, origin, depth + 1)
            }),
            Value::String(id) => match self.fns.store.item_modifiers.get(id) {
                Some(f) => self.apply_item_modifier(f, stack, rng, this, origin, depth + 1),
                None => stack,
            },
            Value::Object(_) => {
                let mut stack = stack?;
                // 26.3 item modifiers: a single `condition`, dispatched on `type`.
                if !m["condition"].is_null() && !self.conditions_hold(&m["condition"], this, origin)
                {
                    return Some(stack);
                }
                match m["type"].as_str().unwrap_or("") {
                    "minecraft:sequence" => {
                        #[allow(clippy::manual_try_fold)]
                        return m["functions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .fold(Some(stack), |s, f| {
                                self.apply_item_modifier(f, s, rng, this, origin, depth + 1)
                            });
                    }
                    "minecraft:set_count" => {
                        let n = int(&m["count"], rng);
                        let base = if m["add"].as_bool() == Some(true) {
                            stack.count as i32
                        } else {
                            0
                        };
                        let max = self
                            .res
                            .items()
                            .get(&stack.item)
                            .map_or(64, |i| i.max_stack);
                        stack.count = (base + n).clamp(0, max as i32) as u32;
                    }
                    "minecraft:limit_count" => {
                        let min = m["limit"]["min"].as_i64().or(m["limit"].as_i64());
                        let max = m["limit"]["max"].as_i64().or(m["limit"].as_i64());
                        if let Some(min) = min {
                            stack.count = stack.count.max(min as u32);
                        }
                        if let Some(max) = max {
                            stack.count = stack.count.min(max as u32);
                        }
                    }
                    "minecraft:set_damage" => {
                        let max = self
                            .res
                            .items()
                            .get(&stack.item)
                            .map_or(0, |i| i.max_damage);
                        if max > 0 {
                            let f = match m["damage"].as_f64() {
                                Some(f) => f as f32,
                                None => {
                                    let lo = m["damage"]["min"].as_f64().unwrap_or(0.) as f32;
                                    let hi = m["damage"]["max"].as_f64().unwrap_or(0.) as f32;
                                    lo + rng.next_float() * (hi - lo)
                                }
                            };
                            let base = if m["add"].as_bool() == Some(true) {
                                1. - stack.damage as f32 / max as f32
                            } else {
                                0.
                            };
                            let frac = (base + f).clamp(0., 1.);
                            stack.damage = ((1. - frac) * max as f32).floor() as u32;
                        }
                    }
                    "minecraft:set_name" => {
                        if m["name"].is_null() {
                            return Some(stack);
                        }
                        if m["target"].as_str() == Some("item_name") {
                            let name = horizon_client::scoreboard::json_to_tag(&m["name"]);
                            edit_components(&mut stack, |c| {
                                c.insert("minecraft:item_name".into(), name);
                            });
                        } else {
                            // SetNameFunction.run preserves the component when no entity resolver is requested.
                            stack
                                .data_mut()
                                .set_loot_name("minecraft:custom_name", &m["name"]);
                        }
                    }
                    "minecraft:set_enchantments" => {
                        let add = m["add"].as_bool() == Some(true);
                        let d = stack.extra.get_or_insert_with(Default::default);
                        let target = if &*stack.item == "minecraft:enchanted_book" {
                            &mut d.stored_enchantments
                        } else {
                            &mut d.enchantments
                        };
                        for (id, level) in m["enchantments"].as_object().into_iter().flatten() {
                            let n = int(level, rng).clamp(0, 255) as u32;
                            let id: std::sync::Arc<str> = if id.contains(':') {
                                id.as_str().into()
                            } else {
                                format!("minecraft:{id}").into()
                            };
                            match target.iter_mut().find(|(e, _)| *e == id) {
                                Some(e) => e.1 = if add { e.1 + n } else { n },
                                None => target.push((id, n)),
                            }
                            target.retain(|(_, l)| *l > 0);
                        }
                    }
                    "minecraft:set_potion" => {
                        if let Some(p) = m["id"].as_str() {
                            stack.extra.get_or_insert_with(Default::default).potion =
                                Some(p.into());
                        }
                    }
                    "minecraft:set_lore" => {
                        let lines: Vec<Tag> = m["lore"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(horizon_client::scoreboard::json_to_tag)
                            .collect();
                        edit_components(&mut stack, |c| {
                            let old: Vec<Tag> = match c.get(&"minecraft:lore".into()) {
                                Some(Tag::List(l)) => l.clone(),
                                _ => Vec::new(),
                            };
                            let new = list_operation(m, old, lines);
                            c.insert("minecraft:lore".into(), Tag::List(new));
                        });
                    }
                    "minecraft:set_custom_data" => {
                        let tag = match &m["tag"] {
                            Value::String(snbt) => horizon_world::nbt::parse_snbt(snbt)
                                .map(|(t, _)| t)
                                .unwrap_or(Tag::Compound(Default::default())),
                            v => horizon_client::scoreboard::json_to_tag(v),
                        };
                        edit_components(&mut stack, |c| {
                            let mut data = match c.get(&"minecraft:custom_data".into()) {
                                Some(t @ Tag::Compound(_)) => t.clone(),
                                _ => Tag::Compound(Default::default()),
                            };
                            merge_compound(&mut data, &tag);
                            c.insert("minecraft:custom_data".into(), data);
                        });
                    }
                    "minecraft:set_components" => {
                        let patch = m["components"].clone();
                        edit_components(&mut stack, |c| {
                            for (k, v) in patch.as_object().into_iter().flatten() {
                                let (remove, key) = match k.strip_prefix('!') {
                                    Some(rest) => (true, rest),
                                    None => (false, k.as_str()),
                                };
                                let key = if key.contains(':') {
                                    key.to_owned()
                                } else {
                                    format!("minecraft:{key}")
                                };
                                if remove {
                                    c.remove(&key.as_str().into());
                                } else {
                                    c.insert(
                                        key.as_str().into(),
                                        horizon_client::scoreboard::json_to_tag(v),
                                    );
                                }
                            }
                        });
                    }
                    "minecraft:toggle_tooltips" => {
                        let toggles = m["toggles"].clone();
                        edit_components(&mut stack, |c| {
                            let mut hidden: Vec<String> = c
                                .get(&"minecraft:tooltip_display".into())
                                .and_then(|d| d.get("hidden_components"))
                                .and_then(|l| match l {
                                    Tag::List(l) => Some(
                                        l.iter()
                                            .filter_map(|t| match t {
                                                Tag::String(s) => Some(s.to_string_lossy()),
                                                _ => None,
                                            })
                                            .collect(),
                                    ),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            let hide_tooltip = c
                                .get(&"minecraft:tooltip_display".into())
                                .and_then(|d| d.get("hide_tooltip"))
                                .cloned();
                            for (k, shown) in toggles.as_object().into_iter().flatten() {
                                let key = if k.contains(':') {
                                    k.clone()
                                } else {
                                    format!("minecraft:{k}")
                                };
                                hidden.retain(|h| *h != key);
                                if shown.as_bool() == Some(false) {
                                    hidden.push(key);
                                }
                            }
                            let mut display: Vec<(&str, Tag)> = Vec::new();
                            if let Some(h) = hide_tooltip {
                                display.push(("hide_tooltip", h));
                            }
                            if !hidden.is_empty() {
                                display.push((
                                    "hidden_components",
                                    Tag::List(
                                        hidden
                                            .iter()
                                            .map(|h| Tag::String(h.as_str().into()))
                                            .collect(),
                                    ),
                                ));
                            }
                            c.insert("minecraft:tooltip_display".into(), Tag::compound(display));
                        });
                    }
                    "minecraft:filtered" => {
                        let pass = self.loot_item_filter(&m["item_filter"], &stack);
                        let branch = if pass { &m["on_pass"] } else { &m["on_fail"] };
                        if branch.is_null() {
                            return Some(stack);
                        }
                        return self.apply_item_modifier(
                            branch,
                            Some(stack),
                            rng,
                            this,
                            origin,
                            depth + 1,
                        );
                    }
                    "minecraft:discard" => stack.count = 0,
                    _ => {}
                }
                (stack.count > 0).then_some(stack)
            }
            _ => stack,
        }
    }
    /// `ItemStack.getMaxStackSize`: the max_stack_size component, else the
    /// item's.
    pub(in crate::game) fn stack_max_size(&self, s: &ItemStack) -> u32 {
        if let Some(Tag::Int(n)) = s.data().and_then(|d| d.other("minecraft:max_stack_size")) {
            return (*n).clamp(1, 99) as u32;
        }
        self.items().get(&s.item).map_or(64, |i| i.max_stack)
    }
    /// `ResourceOrIdArgument.getItemModifier`: an inline modifier (SNBT) or
    /// a registered one.
    pub(super) fn item_modifier_arg(&mut self, ctx: &Context, name: &str) -> R<serde_json::Value> {
        let raw = match ctx.get(name) {
            Some(Arg::Resource(s) | Arg::Str(s)) => s.clone(),
            _ => return Err(err("command.failed", &[])),
        };
        if raw.starts_with('{') || raw.starts_with('[') {
            let (tag, _) = horizon_world::nbt::parse_snbt(&raw)
                .map_err(|e| err("argument.nbt.invalid", &[&e]))?;
            return Ok(horizon_client::scoreboard::tag_to_json(&tag));
        }
        let id = if raw.contains(':') {
            raw
        } else {
            format!("minecraft:{raw}")
        };
        self.fns
            .store
            .item_modifiers
            .get(&id)
            .cloned()
            .ok_or_else(|| err("argument.item_modifier.unknown", &[&id]))
    }
    /// ItemCommands.applyModifier.
    fn modified(&mut self, ctx: &Context, stack: Option<ItemStack>) -> R<Option<ItemStack>> {
        let modifier = self.item_modifier_arg(ctx, "modifier")?;
        let source = self.command_source.unwrap_or_else(|| self.player_source());
        let view = source.entity.map(|t| self.entity_view_of(t));
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        let mut rng = horizon_core::random::LegacyRandom::new(seed);
        let new =
            self.apply_item_modifier(&modifier, stack, &mut rng, view.as_ref(), source.pos, 0);
        // ItemStack.limitSize to the (new) max stack size.
        Ok(new.map(|mut s| {
            s.count = s.count.min(self.stack_max_size(&s));
            s
        }))
    }
    /// ItemCommands.modifyBlockItem / modifyEntityItem.
    pub(super) fn cmd_item_modify(&mut self, ctx: &Context) -> R<i32> {
        self.ensure_datapacks();
        let Some(Arg::Str(slot)) = ctx.get("slot").cloned() else {
            return Ok(0);
        };
        let targets = self.owner_of(ctx, "targets", false)?;
        let mut changed: Vec<(Owner, Option<ItemStack>)> = Vec::new();
        for o in targets {
            if let Owner::Block(p) = o {
                self.container_at(p, "commands.item.target.not_a_container")?;
            }
            let Some(s) = self.one_slot(&o, &slot) else {
                if matches!(o, Owner::Block(_)) {
                    return Err(err("commands.item.target.no_such_slot", &[&slot]));
                }
                continue;
            };
            let current = self.slot_get(&s);
            let stack = self.modified(ctx, current)?;
            if self.slot_set(&s, stack.clone()) {
                changed.push((o, stack));
            }
        }
        self.item_set_feedback(changed, None, &slot)
    }
    /// ItemCommands `replace ... with <item> [<count>]` and `replace ... from
    /// (block|entity) <source> <sourceSlot> [<modifier>]`.
    pub(super) fn cmd_item(&mut self, ctx: &Context) -> R<i32> {
        let Some(Arg::Str(slot)) = ctx.get("slot").cloned() else {
            return Ok(0);
        };
        let (stack, known) = if ctx.has("with") {
            let stack = self.item_from_input(ctx)?;
            (stack.clone(), stack)
        } else {
            let Some(Arg::Str(source_slot)) = ctx.get("sourceSlot").cloned() else {
                return Ok(0);
            };
            let source = self.owner_of(ctx, "source", true)?;
            let o = source
                .first()
                .cloned()
                .ok_or_else(|| err("argument.entity.notfound.entity", &[]))?;
            if let Owner::Block(p) = o {
                self.container_at(p, "commands.item.source.not_a_container")?;
            }
            let s = self
                .one_slot(&o, &source_slot)
                .ok_or_else(|| err("commands.item.source.no_such_slot", &[&source_slot]))?;
            let mut stack = self.slot_get(&s);
            if ctx.get("modifier").is_some() {
                self.ensure_datapacks();
                stack = self.modified(ctx, stack)?;
            }
            (stack.clone(), stack)
        };
        let targets = self.owner_of(ctx, "targets", false)?;
        let mut changed: Vec<(Owner, Option<ItemStack>)> = Vec::new();
        for o in targets {
            if let Owner::Block(p) = o {
                self.container_at(p, "commands.item.target.not_a_container")?;
            }
            let Some(s) = self.one_slot(&o, &slot) else {
                if matches!(o, Owner::Block(_)) {
                    return Err(err("commands.item.target.no_such_slot", &[&slot]));
                }
                continue;
            };
            if self.slot_set(&s, stack.clone()) {
                changed.push((o, stack.clone()));
            }
        }
        self.item_set_feedback(changed, Some(known), &slot)
    }
    /// ItemCommands' `set.success` feedback: a block's slot, one entity's or
    /// several; `known` is the stack every target was given (setEntityItem).
    fn item_set_feedback(
        &mut self,
        changed: Vec<(Owner, Option<ItemStack>)>,
        known: Option<Option<ItemStack>>,
        slot: &str,
    ) -> R<i32> {
        let empty = ItemStack {
            item: "minecraft:air".into(),
            count: 0,
            damage: 0,
            extra: None,
        };
        let name = |g: &Self, s: &Option<ItemStack>| g.stack_name(s.as_ref().unwrap_or(&empty));
        let message = match &changed[..] {
            [] => {
                return Err(match &known {
                    Some(k) => CommandError::new(Text::translate(
                        "commands.item.target.no_changed.known_item",
                        &[name(self, k), Text::literal(slot)],
                    )),
                    None => err("commands.item.target.no_changes", &[&slot]),
                });
            }
            [(Owner::Block(p), stack)] => Text::translate(
                "commands.item.block.set.success",
                &[
                    Text::literal(p[0].to_string()),
                    Text::literal(p[1].to_string()),
                    Text::literal(p[2].to_string()),
                    name(self, stack),
                ],
            ),
            [(o, stack)] => Text::translate(
                "commands.item.entity.set.success.single",
                &[self.owner_name(o), name(self, stack)],
            ),
            [(_, stack), ..] => Text::translate(
                "commands.item.entity.set.success.multiple",
                &[
                    Text::literal(changed.len().to_string()),
                    name(self, known.as_ref().unwrap_or(stack)),
                ],
            ),
        };
        self.feedback(message);
        Ok(changed.len() as i32)
    }
    /// ExecuteCommand.countItems: the matching items' count in the slots.
    pub(super) fn condition_items(&mut self, c: &Context) -> R<(bool, i32)> {
        let owners = self.owner_of(c, "entities", false)?;
        if let [Owner::Block(p)] = owners[..] {
            self.container_at(p, "commands.item.source.not_a_container")?;
        }
        let Some(Arg::Str(slot_name)) = c.get("slots").cloned() else {
            return Ok((false, 0));
        };
        let Some(Arg::Item(predicate)) = c.get("item_predicate").cloned() else {
            return Ok((false, 0));
        };
        let mut count = 0;
        for o in owners {
            for s in self.slots_named(&o, &slot_name) {
                if let Some(stack) = self.slot_get(&s)
                    && self.stack_matches(&predicate, &stack)
                {
                    count += stack.count as i32;
                }
            }
        }
        Ok((count > 0, count))
    }
}

/// Edit a stack's components as their saved compound.
fn edit_components(stack: &mut ItemStack, f: impl FnOnce(&mut horizon_world::nbt::Compound)) {
    let mut c = match horizon_client::item_data::encode_components(stack) {
        Some(Tag::Compound(c)) => c,
        _ => horizon_world::nbt::Compound::new(),
    };
    f(&mut c);
    let tag = Tag::Compound(c);
    let (damage, extra) = horizon_client::item_data::decode_components(Some(&tag));
    stack.damage = damage;
    stack.extra = extra;
    stack.tidy();
}
/// `CompoundTag.merge`: nested compounds merge, other values replace.
fn merge_compound(into: &mut Tag, from: &Tag) {
    let (Tag::Compound(a), Tag::Compound(b)) = (into, from) else {
        return;
    };
    for (k, v) in b {
        match (a.get_mut(k), v) {
            (Some(existing @ Tag::Compound(_)), Tag::Compound(_)) => merge_compound(existing, v),
            _ => {
                a.insert(k.clone(), v.clone());
            }
        }
    }
}
/// `ListOperation.apply` (`mode`: replace_all, replace_section, insert,
/// append) with a 256-entry limit.
fn list_operation(m: &serde_json::Value, old: Vec<Tag>, new: Vec<Tag>) -> Vec<Tag> {
    let offset = m["offset"].as_u64().unwrap_or(0) as usize;
    let mut out = match m["mode"].as_str().unwrap_or("") {
        "replace_all" => new,
        "insert" => {
            let at = offset.min(old.len());
            let mut out = old[..at].to_vec();
            out.extend(new);
            out.extend_from_slice(&old[at..]);
            out
        }
        "replace_section" => {
            let at = offset.min(old.len());
            let size = m["size"].as_u64().map_or(new.len(), |s| s as usize);
            let mut out = old[..at].to_vec();
            out.extend(new);
            let end = (at + size).min(old.len());
            out.extend_from_slice(&old[end..]);
            out
        }
        _ => {
            let mut out = old;
            out.extend(new);
            out
        }
    };
    out.truncate(256);
    out
}
