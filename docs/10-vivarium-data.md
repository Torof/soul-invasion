# The vivarium's data: the report as JSON

Since 22 November 2026 (D41 amended, D52), the vivarium writes its report twice: the HTML page
(`--out FILE`, the project lead reads it) and, beside it, the same report as data, `FILE` with
`.json` in place of its extension (`report.html` → `report.json`). The laboratory (`tools/lab/`)
reads the data and never parses the page.

The file is written by `server/vivarium/src/json.rs` (`Report::json`), the only writer. Objects
are on their own lines, arrays inline, so it reads and stays small: a run of two tribes of 25 over
twenty years weighs some 300 KB of data, plus the map's picture (`map.data_uri`, about 760 KB on
the vivarium's world: the PNG is stored uncompressed, as on the page), against several megabytes
for its page, and the bodies' positions at noon (`positions`, one day a line) some 20 bytes a body
a day: 50 bodies over twenty years weigh about a megabyte. Nothing in it is per tick: the curves
are the samples the page already thins to at most 1,200 points, and the presence grid and the
positions take the living once a day. **The chronicle is not in the file**: it is on the page,
and the people's own lines of life are under `people`.

Everything is deterministic (the same settings give the same file, byte for byte) but
`meta.timing`: the wall and birth seconds, and `run_date`, the date of the run read from the OS
clock, the only field no seed decides.

## Conventions

- `format` is raised when a key changes meaning or goes away; a new key does not raise it.
- Days count from 1 (the run's first day); ticks from 0. A year is `meta.days_per_year` days.
- Tribes are named by their letter, `"A"` or `"B"`, in the order of `meta.camps`; the arrays "by
  tribe" under `curves` follow the same order.
- Positions are `[x, y]` in tiles of one metre, `x` growing east and `y` south.
- Food is in kilojoules (a person needs about 10,000 a day). "Hours" are person-hours: the gauges
  are sampled once a game hour, 24 a person-day, so `thin_hours / 24` is the page's "days thin".
- A key whose value may be `null` is marked so. A test (`the_data_holds_the_documented_keys_and_no_other`)
  walks every key below against a run and every key of a run against this table: `[]` stands for
  every element of an array, `*` for any key of an object.

## The keys

| Key | Meaning | Unit or form |
|---|---|---|
| `meta.format` | The format marker, `1` since 22 November 2026 | integer |
| `meta.settings.world` | The world | `"vivarium"` or `"patch"` |
| `meta.settings.side` | A patch's side; `null` on the vivarium's world | tiles |
| `meta.settings.people` | Grown people born at each camp | count |
| `meta.settings.children` | Children born at each camp | count |
| `meta.settings.infants` | Infants born at each camp | count |
| `meta.settings.adult_years` | The grown people's ages at the start, first to second | `[years, years]` |
| `meta.settings.fed` | Food without limit (`--fed`) | boolean |
| `meta.settings.brain` | The people think (false: the control run without a brain) | boolean |
| `meta.settings.tribes` | Tribes, 1 or 2 | count |
| `meta.settings.days` | Game days run | days |
| `meta.settings.ticks_per_second` | The cadence: ticks a game second (the game runs at 10) | ticks |
| `meta.settings.seed` | The run's seed | integer |
| `meta.settings.trials` | The brains try, judge and copy their ways (false: `--no-trials`) | boolean |
| `meta.settings.fire` | The evening fire and the common memory, D55 (false: `--no-fire`); since 23 November 2026 | boolean |
| `meta.settings.content` | The content directory given, its last component; `null` when found from the working directory | string |
| `meta.map_tiles` | The world's size | `[width, height]` tiles |
| `meta.camps` | The camps' starting positions, A first | `[[x, y], …]` |
| `meta.days_per_year` | The length of a year | days |
| `meta.ticks_per_day` | The length of a day at this cadence | ticks |
| `meta.ticks` | Ticks run | ticks |
| `meta.rates[].name` | A rate of `content/needs.ron` the people live by | string |
| `meta.rates[].rate` | Its value | points of 100 a game day |
| `meta.timing.wall_s` | The run's wall time | seconds |
| `meta.timing.birth_s` | The world's birth | seconds |
| `meta.timing.run_date` | When the run was written, from the OS clock | `"2026-11-22T15:04:00Z"`, UTC |
| `meta.timing.helpers` | Helper threads placing the chunks' plants ahead of the bodies (`--helpers`, D51 amended); 0 is the simulation thread alone, what the lab passes for runs side by side. Timing: the pages are the same whatever the number | count |
| `meta.timing.placed` | Chunks whose plants were placed over the run, by the helpers and the simulation thread together (a chunk twice when both placed it at once) | count |
| `meta.timing.placed_ahead` | Of those, the chunks the helpers placed ahead | count |
| `meta.memory.written` | Records written to the common memory over the run (D55) | count |
| `meta.memory.freed` | Records freed, their last access dropped | count |
| `meta.memory.held` | Records held at the end, those of `records` | count |
| `years[].tribe` | The tribe | `"A"` or `"B"` |
| `years[].year` | The year, from 1 | integer |
| `years[].first_day` | Its first day | day |
| `years[].last_day` | Its last day, or the run's | day |
| `years[].population` | Alive at the year's end | count |
| `years[].population_before` | Alive at the year's start | count |
| `years[].growth_permille` | The growth over the year, (end − start) × 1000 / start, rounded towards zero; `null` when nobody was alive at the start | per mille |
| `years[].births` | Born in the year | count |
| `years[].deaths.*` | Died in the year, by cause: `illness`, `old_age`, `childbirth`, `hunger`, `thirst`, `cold` | count |
| `years[].deaths_total` | Died in the year | count |
| `years[].hunger_seasons` | The season of each death of hunger | `["winter", …]` |
| `years[].intervals_days` | Each birth's interval since its mother's birth before it | `[days, …]` |
| `years[].interval_days_mean` | Their mean, rounded down; `null` without any | days |
| `years[].fifteen.turned` | Children born in the run who turned fifteen by the year's end | count |
| `years[].fifteen.alive` | How many of them were alive to | count |
| `years[].fertile_women.total` | Women of 16 to 42 alive at the year's end | count |
| `years[].fertile_women.without_spouse` | Of them, without a living spouse and without a man of the tribe they could take | count |
| `years[].trials` | Trials started in the year (D50) | count |
| `years[].kept` | Trials judged in the year and kept | count |
| `years[].copied` | Ways copied from camp-mates in the year | count |
| `years[].homes_moved` | Homes moved in the year, by a trial or a copy | count |
| `years[].home_moves[].day` | A home moved in the year (from the brain's news): the day | day |
| `years[].home_moves[].body` | Whose | integer |
| `years[].home_moves[].from` | The home before | `[x, y]` |
| `years[].home_moves[].to` | The home after | `[x, y]` |
| `years[].home_moves[].how` | `"trial"` (a trial of the home starts), `"copy"` (copied from a camp-mate), `"reverted"` (the trial given up: the home goes back, not counted in `homes_moved`) | string |
| `years[].ways.minds` | Minds read at the year's end among the living; the whole object is `null` when none | count |
| `years[].ways.sleep_out` | How many of them sleep out | count |
| `years[].ways.median_worth_tiles` | The median of how far a plant's worth is halved | tiles |
| `years[].ways.median_stock_kj` | The median of what they store before resting | kJ |
| `years[].ways.stock_kj.q1` | The first quartile of what they store before resting (D55: the lead's metric) | kJ |
| `years[].ways.stock_kj.median` | Its median, the same as `median_stock_kj` | kJ |
| `years[].ways.stock_kj.q3` | Its third quartile | kJ |
| `years[].camp` | The camp at the year's end, the median home of the living; `null` when no mind was read | `[x, y]` |
| `years[].food.start_kj` | Carried by the tribe's living at the year's start; the whole object is `null` for a year the run did not reach | kJ |
| `years[].food.gathered_kj` | Gathered in the year | kJ |
| `years[].food.eaten_kj` | Eaten | kJ |
| `years[].food.spoiled_kj` | Spoiled while carried | kJ |
| `years[].food.end_kj` | Carried at the year's end (or the run's) | kJ |
| `years[].food.lost_kj` | What the flows do not tell: left on the ground by the dead | kJ |
| `years[].food.thin_hours` | A living person thin (fat reserve under half) | person-hours |
| `years[].food.weak_hours` | A living person weak (under a fifth) | person-hours |
| `years[].food.stored_kj.noon_mean` | Carried by the tribe's living at noon, the mean of the year's noons (D55: the stored food, the lead's metric); `null` without a noon | kJ |
| `years[].food.stored_kj.noon_min` | The least carried at a noon of the year | kJ |
| `years[].food.stored_kj.end` | Carried at the year's end, the same as `end_kj` | kJ |
| `years[].fire.nights` | Nights with something told at the tribe's fire in the year (D55) | count |
| `years[].fire.told` | Records told at it over the year | count |
| `years[].fire.heard` | Records heard and held, one per listener | count |
| `years[].fire.dispelled` | Rumours dispelled in the year: a body went where a teller had said and found nothing | count |
| `years[].knowledge.places` | Distinct places some living member of the tribe knew of at the year's end, seen (D53) or heard of (D55); the whole object is `null` when no mind was read | count |
| `years[].knowledge.recorded` | Of them, places some living member holds an access to a record of (D55): told at a fire, or heard | count |
| `years[].knowledge.heard_only` | Of them, places no living member saw itself: known by hearsay only | count |
| `years[].knowledge.accesses` | Accesses to places held by the living, all places and bodies | count |
| `years[].knowledge.accesses_heard` | Of them, accesses got from a teller | count |
| `found[].tribe` | The tribe | `"A"` or `"B"` |
| `found[].tried.*` | Trials started over the run, by the kind of the step: `wider`, `narrower`, `sleep_out`, `sleep_home`, `store_more`, `store_less`, `home_moved` | count |
| `found[].kept.*` | Trials judged and kept, by kind | count |
| `found[].copied.*` | Ways copied from camp-mates, by kind | count |
| `found[].heard.trials` | Trials whose step was a kept way heard at the fire (D55) | count |
| `found[].heard.copies` | Ways copied by hearsay: with the fire lit every copy, without it none (the control copies by proximity) | count |
| `found[].heard.dispelled` | Rumours dispelled over the run | count |
| `found[].by_will[].band` | Trials by the will of the body when it started (D50, amended): the band, `timid` (0 to 20), `middling` (21 to 50) or `bold` (51 to 100) | string |
| `found[].by_will[].from` | The band's lowest will | 0 to 100 |
| `found[].by_will[].to` | Its highest | 0 to 100 |
| `found[].by_will[].tried` | Trials started by bodies of that will | count |
| `found[].by_will[].kept` | Of them, judged and kept | count |
| `people[].id` | The person's number, the page's "person N" | integer |
| `people[].name` | As the page names it | `"person N"` |
| `people[].tribe` | Its tribe | `"A"` or `"B"` |
| `people[].sex` | `"woman"`, `"man"`, or `null` on a world without life | string |
| `people[].born` | `"start"` for those born at the departure, else its birth day | `"start"` or day |
| `people[].born_day` | Its birth day; 1 for those born before the world | day |
| `people[].age_days_at_start` | Its age on day one, for those born at the departure; `null` for those born in the run | days |
| `people[].born_at` | Where it was born; `null` if unknown | `[x, y]` |
| `people[].died.day` | The day it died; the whole object is `null` while alive at the end | day |
| `people[].died.tick` | The tick | tick |
| `people[].died.cause` | Of what: `illness`, `old_age`, `childbirth`, `hunger`, `thirst`, `cold` | string |
| `people[].died.season` | The season | `"winter"` |
| `people[].died.at` | Where | `[x, y]` |
| `people[].mother` | Its mother's id; `null` when unknown | integer |
| `people[].father` | Its father's id; `null` when unknown | integer |
| `people[].spouses[].day` | The day a spouse was taken | day |
| `people[].spouses[].id` | That spouse | integer |
| `people[].children` | Children whose mother or father it is | count |
| `people[].lost` | The days it lost an unborn child | `[day, …]` |
| `people[].will` | Its will (D50), once its mind was read; `null` before | 0 to 100 |
| `people[].will_start` | Its will when its mind was first read: the innate (D50, amended); `null` for a mind never read | 0 to 100 |
| `people[].believes[].place` | A place it held an access to at the end, or at its death (D55): the place's key, its 16-tile square | `[x, y]` |
| `people[].believes[].record` | The record it believes of it | integer |
| `people[].believes[].seen` | Whether it saw it itself (false: heard) | boolean |
| `people[].believes[].by` | Who it has it from: itself, or the teller | integer |
| `people[].believes[].day` | The day it got it | day |
| `people[].ways.worth_half_tiles` | Its ways at the end, or at its death (D50); the whole object is `null` for a mind never read | tiles |
| `people[].ways.sleep_out` | Whether it sleeps out | boolean |
| `people[].ways.stock_kj` | What it stores before resting | kJ |
| `people[].ways.home` | Its home | `[x, y]` |
| `people[].trials[].day` | A trial's first day | day |
| `people[].trials[].step` | The kind of the step, as `found` names them | string |
| `people[].trials[].words` | The step in words, as the page tells it | string |
| `people[].trials[].judged_day` | The day the world judged it; `null` while under trial or if it died first | day |
| `people[].trials[].kept` | Whether it was kept; `null` while unjudged | boolean |
| `people[].trials[].will` | Its will when it started the trial; `null` for a mind not read then | 0 to 100 |
| `people[].trials[].heard` | Whether the step was a kept way heard at the fire (D55) | boolean |
| `people[].copied` | Ways it copied from camp-mates | count |
| `people[].thin_hours` | Hours its fat reserve was under half | person-hours |
| `people[].weak_hours` | Hours it was under a fifth | person-hours |
| `people[].gathered_kj` | What it gathered over the run | kJ |
| `people[].meals` | Meals eaten | count |
| `people[].drinks` | Times it drank | count |
| `people[].thoughts` | Times its brain thought | count |
| `people[].lines[].day` | A line of its life: its birth, its spouse, its children, their weaning, a lost child, its trials, its death; never a thought, a meal, a drink or a gather | day |
| `people[].lines[].tick` | The tick | tick |
| `people[].lines[].what` | The line, as the chronicle says it | string |
| `records[].id` | A record of the common memory at the end (D55): its number | integer |
| `records[].kind` | `place`, `water`, `trial` or `night` | string |
| `records[].day` | Its day | day |
| `records[].witness` | Who saw it first, or the trial's body; `null` for a night | integer |
| `records[].refs` | Accesses and nights holding it | count |
| `records[].place` | A place record: the place's key; `null` for another kind, as every key of one kind is | `[x, y]` |
| `records[].states` | Its states, each a kind of food and the bucket of plants bearing it (`place_buckets`) | `[[kind, bucket], …]` |
| `records[].way` | A trial record: the way, `worth_half_tiles`, `sleep_out`, `stock_kj` or `home` | string |
| `records[].up` | Whether the step went up (wider, out, more; a home moved is up) | boolean |
| `records[].kept` | Whether the trial was kept | boolean |
| `records[].will` | The body's will when it was judged | 0 to 100 |
| `records[].present` | A night record: how many lay down | count |
| `records[].told` | How many records were told | count |
| `hearth[].tribe` | The nights a tribe's hearth still holds at the end (the last `hearth_nights`, D55): the tribe | `"A"` or `"B"` |
| `hearth[].nights[].day` | A night: the day that closed with it | day |
| `hearth[].nights[].camp` | The camp | `[x, y]` |
| `hearth[].nights[].present` | How many lay down | count |
| `hearth[].nights[].told[].record` | A record told | integer |
| `hearth[].nights[].told[].by` | Its teller | integer |
| `rumours[].day` | A rumour dispelled (D55): the day | day |
| `rumours[].tribe` | The tribe of who went | `"A"` or `"B"` |
| `rumours[].body` | Who went where a teller had said and found nothing | integer |
| `rumours[].place` | The place | `[x, y]` |
| `rumours[].record` | The record it had believed | integer |
| `rumours[].from` | Who had told it | integer |
| `curves.points` | Samples kept, at most 1,200 | count |
| `curves.every` | One sample kept every so many of the 24 a day | count |
| `curves.ticks` | The tick of each sample | `[tick, …]` |
| `curves.days` | Its day | `[day, …]` |
| `curves.alive` | Bodies alive | `[count, …]` |
| `curves.alive_by_tribe` | The same by tribe, in the order of the camps | `[[count, …], …]` |
| `curves.thin_by_tribe` | The living thin, by tribe | `[[count, …], …]` |
| `curves.weak_by_tribe` | The living weak, by tribe | `[[count, …], …]` |
| `curves.reserve_by_tribe` | The mean fat reserve of the living, by tribe | `[[percent of full, …], …]` |
| `curves.hunger` | Mean hunger of the living | `[points of 100, …]` |
| `curves.thirst` | Mean thirst | `[points, …]` |
| `curves.fatigue` | Mean fatigue | `[points, …]` |
| `presence[].tribe` | Where a tribe's living were over a year: the tribe | `"A"` or `"B"` |
| `presence[].year` | The year | integer |
| `presence[].cell_tiles` | The side of a cell, 32 | tiles |
| `presence[].cells` | Each cell where somebody stood at noon, with the count of living bodies there over the year's noons (one a day); cells at zero omitted; `cx = x / 32`, `cy = y / 32` | `[[cx, cy, count], …]` |
| `positions` | Where every living body stood at noon, day by day (the lab's map of the movements): each day in order, with its living bodies in id order, each `[id, x, y]`; a body dead by noon is not there, so a day's list is the living of `people` that noon. One day a line in the file | `[[day, [[id, x, y], …]], …]` |
| `map.data_uri` | The page's picture of the map at the run's end, the same PNG: the terrain, the camps red, the living white, the dead black | `"data:image/png;base64,…"` |
| `map.tiles_per_pixel` | The scale: one pixel is so many tiles a side (5 on the vivarium's world) | tiles |
| `map.tiles` | The world's size | `[width, height]` tiles |
| `map.pixels` | The picture's size | `[width, height]` pixels |
| `map.legend.*` | The colours of the dots: `camp`, `alive`, `dead` | `[r, g, b]` |

## Not in the file

- The chronicle (on the page), the map's picture, the families' pages (worked out from `people`:
  mother, father, spouses, children), the ages table (worked out from `born_day` and `sex`).
- The camp's median position per day: the vivarium works it out once a year (`years[].camp`), not
  per day; the lab can take it from `positions` (the median of a tribe's bodies at noon). The
  presence grid (`presence`) and the positions (`positions`) are the two things sampled for the
  data alone, once a day at noon: where the living of each tribe were, and each body one by one.
