# The vivarium's data: the report as JSON

Since 22 November 2026 (D41 amended, D52), the vivarium writes its report twice: the HTML page
(`--out FILE`) and, beside it, the same report as data, `FILE` with `.json` in place of its
extension (`report.html` → `report.json`). The laboratory (`tools/lab/`) reads the data and never
parses the page. Since lab 6 (23 November 2026) the project lead reads the lab's run page; the
vivarium's own page is a minimal technical one (settings, end counts, the map, the timing), and
`--page full` writes the former long page.

The file is written by `server/vivarium/src/json.rs` (`Report::json`), the only writer. Objects
are on their own lines, arrays inline, so it reads and stays small: a run of two tribes of 25 over
twenty years weighs some 300 KB of data, plus the food and relief maps (some 100 and 200 KB),
plus the map's picture (`map.data_uri`, about 760 KB on
the vivarium's world: the PNG is stored uncompressed, as on the page), against several megabytes
for its page, and the bodies' positions at noon (`positions`, one day a line) some 20 bytes a body
a day: 50 bodies over twenty years weigh about a megabyte. Nothing in it is per tick: the curves
are the samples the page already thins to at most 1,200 points, and the presence grid and the
positions take the living once a day. **The chronicle is not in the file**: it is on the full page,
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
| `meta.settings.decisions` | The decisions at the evening fire, D56 (false: `--no-decisions`); since 23 November 2026 | boolean |
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
| `meta.factors[].key` | A value of `content/needs.ron` by its path in the file, objects walked into (`trials.trial_days`, `night_from_percent.winter`); the lab's factor cards (lab 6) read them by key, an unknown key shown as it is | string |
| `meta.factors[].value` | Its value as written in the content: a number, a text, a list; a list of objects kept whole as its JSON text | as in the content |
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
| `years[].ways.home_grade_permille.min` | How the tribe's living grade their own home by their notebook at the year's end (D53, amended 24 November 2026), the lowest; the whole object `null` when none slept at home | per mille of a day of need |
| `years[].ways.home_grade_permille.median` | Its median | per mille of a day of need |
| `years[].ways.home_grade_permille.max` | Its highest | per mille of a day of need |
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
| `years[].knowledge.where` | The places themselves (since 23 November 2026, the lab's map): each place's key, its 16-tile square, and 1 when some living member saw it itself, 0 when known by hearsay only; in key order, the first 1,000 (`places` counts them all) | `[[x, y, seen], …]` |
| `years[].decisions.evenings` | Evenings the tribe's fire said decisions (D56): two or more lay down at the camp | count |
| `years[].decisions.full_told_evenings` | Of them, evenings someone told a place full (the look out is drawn half as often then) | count |
| `years[].decisions.said.*` | What each present said at the fire, by what: `alone` (went alone to a place of its choice, D56), `called` (called others to a place it saw), `joined` (joined a call), `looked` (said it goes looking), `nothing` (no plan: absent from the fire, a child without its mother there, one carrying enough, or the decisions off), `moves` (called a move to the home its notebook graded best, D56 amended 24 November 2026; who joined it counts in `joined`) | count |
| `years[].decisions.calls_full` | Calls that filled: as many joined as the caller saw room for | count |
| `years[].decisions.calls_skipped` | Choices passed over because the place was already taken whole by a call or by those going alone | count |
| `years[].decisions.absent` | Members of the camp not at its fire on an evening of decisions, summed over the evenings | person-evenings |
| `years[].decisions.groups.formed` | Groups formed at the fire: a caller and one or more who joined | count |
| `years[].decisions.groups.members` | Their members, the callers counted | count |
| `years[].decisions.groups.followed` | Members who walked behind their caller at least once that day (`Follow`, on the way there or home) | count |
| `years[].decisions.groups.full` | Members who carried what they wanted before the group went home | count |
| `years[].decisions.groups.left` | Members who left their group on the way (lost sight of the caller, or the place out of reach) | count |
| `years[].decisions.groups.ended.*` | How the groups ended: `home` (the caller called them home), `bare` (the place bare and nothing to go on to), `night` (night came first, or the caller died), `blocked` (the caller could not walk to the place: seen, not reached, D53 amended 24 November 2026; told as `bare` before) | count |
| `years[].decisions.barred.*` | Marks of seen, not reached (D53, amended 24 November 2026): `made` (a walk barred by water or rock), `heard` (at the fire), `cleared_stood` (the body stood on the place itself: nothing else clears a mark) | count |
| `years[].decisions.barred.held` | Marks of seen, not reached held by the tribe's living at the year's end (the last year's is the run's end) | count |
| `years[].decisions.barred.causes.*` | What barred the walks that made a mark in the year: `water` or `rock` (the cliff or other ground no body enters); a walk stopped with no way in sight makes no mark | count |
| `years[].decisions.groups.moved_on` | Groups that found their place bare and went on to the caller's next | count |
| `years[].decisions.groups.stretch_ticks.mean` | The stretch of a group: from its caller's arrival at the place to its last member's; the mean over the groups where both arrived; `null` without one | ticks |
| `years[].decisions.groups.stretch_ticks.max` | The longest | ticks |
| `years[].decisions.groups.stretch_ticks.groups` | The groups measured | count |
| `years[].decisions.groups.together` | Members who stood at the place within a minute of their caller (the caller not counted): those who walked with it | count |
| `years[].decisions.groups.waited_ticks` | Ticks the callers waited on the way for a member left behind ("look behind", D56) | ticks |
| `years[].decisions.groups.waited_groups` | Groups whose caller waited so | count |
| `years[].decisions.days_by_act.*` | Days lived by adults and children with a day of their own, by what they had said (the keys of `said`) | person-days |
| `years[].decisions.trips.found.*` | Places found: seen by one and told at the fire to a listener who did not know them, by what the finder did that day | count |
| `years[].decisions.trips.bare.*` | Places gone to and found bare, by what was done that day | count |
| `years[].decisions.trips.fed_days.*` | Days one gathered a day of its need or more, by what was done that day | person-days |
| `years[].decisions.wills.minds` | The tribe's living whose mind was read at the year's end (D50, amended) | count |
| `years[].decisions.wills.bold` | Of them, the bold: a will of `temperance.bold_will` (40) or more | count |
| `years[].decisions.judged.successes.*` | Days of each activity judged a success in the year (D56): `explore` (a look out that found a place told new), `gather` (a walk to a place that gathered a day of need), `stay` (a silence at the fire with its children fed) | person-days |
| `years[].decisions.judged.failures.*` | And judged a failure | person-days |
| `years[].decisions.review.named` | Companions a caller called first, by name, at the evening's review (D56, PR 1 ter): its last outings with them good | count |
| `years[].decisions.review.set_aside` | Companions told "not with us tomorrow": bad on enough of their last outings with that caller (waited for, or back with less than a day of need) | count |
| `years[].decisions.review.refused` | Calls one set aside would have joined, refused | count |
| `years[].decisions.review.weighed_joins` | Joins weighed by a reputation: the call of whom one believes best at gathering taken first | count |
| `years[].decisions.review.asked` | Questions asked at the fire ("where is food?") by one with no place to go | count |
| `years[].decisions.review.answered` | Of them, answered with a place it can go to | count |
| `years[].decisions.review.said_nothing.stocked` | Who said nothing at the fire because it carries its stock and a day's need (D56, PR 1 ter: a measure of the « nothing » days) | count |
| `years[].decisions.review.said_nothing.no_place` | Who said nothing knowing no place bearing now | count |
| `years[].decisions.review.said_nothing.all_taken` | Who said nothing, every place it weighed taken whole (after a question, if any) | count |
| `years[].decisions.need_percent_by_act.*` | What was gathered on a day, in per cent of a day of the need, on average, by what had been said; `null` without a day | per cent |
| `years[].decisions.review.set_aside_days.*` | What those set aside did that day, by act (`alone`, `called`, `joined`, `looked`, `nothing`, `moves`) | person-days |
| `years[].decisions.review.set_aside_fed_days.*` | Of them, the days they gathered a day of their need | person-days |
| `years[].decisions.follows.*.days` | Days one followed another (D56, PR 1 ter): `joined` its call, `asked` it and went where it answered, `told` (went alone to a place another found and told) | person-days |
| `years[].decisions.follows.*.fed` | Of them, back with a day of its need | person-days |
| `years[].decisions.follows.*.empty` | Of them, back with nothing | person-days |
| `years[].decisions.follows.*.mean_need_percent` | What the follower gathered, in per cent of a day of its need, on average; `null` without a day | per cent |
| `years[].reputed.*[]` | Who the tribe's living believed best at the year's end (D56, PR 1 ter), by activity (`explore`: the scout; `gather`: the one whose calls feed), the most believed first, three at most; `null` when nobody is reputed. A belief each holds, never a title | list |
| `years[].reputed.*[].who` | The one believed best | id |
| `years[].reputed.*[].believers` | How many of the living believed it | count |
| `years[].reputed.*[].mean_credit` | Their credit in it, on average (1 to 100) | points |
| `years[].decisions.distance_noon_tiles.mothers` | Mean distance at noon from its camp, on each axis the larger, of an adult (16 years or more) carrying an infant; `null` without one | tiles |
| `years[].decisions.distance_noon_tiles.mother_noons` | The noons measured | count |
| `years[].decisions.distance_noon_tiles.others` | The same for the other adults | tiles |
| `years[].decisions.distance_noon_tiles.other_noons` | The noons measured | count |
| `found[].tribe` | The tribe | `"A"` or `"B"` |
| `found[].tried.*` | Trials started over the run, by the kind of the step: `wider`, `narrower`, `sleep_out`, `sleep_home`, `store_more`, `store_less`, `home_moved` | count |
| `found[].kept.*` | Trials judged and kept, by kind | count |
| `found[].copied.*` | Ways copied from camp-mates, by kind | count |
| `found[].heard.trials` | Trials whose step was a kept way heard at the fire (D55) | count |
| `found[].heard.copies` | Ways copied by hearsay: with the fire lit every copy, without it none (the control copies by proximity) | count |
| `found[].heard.dispelled` | Rumours dispelled over the run, every one: the sum of the tribe's `years[].fire.dispelled` (before 23 November 2026 it counted only the rumours kept in `rumours[]`, at most 1,000) | count |
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
| `people[].company.sociable` | Its innate sociability (D53, amended 24 November 2026), once a night of it was felt; `null` before | 0 to 100 |
| `people[].company.nights` | Nights it felt company or its lack, at dawn, the fire lit (an infant feels none) | count |
| `people[].company.alone` | Of them, the nights at no fire: slept out, or alone at its camp | count |
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
| `people[].trials[].grades_permille` | A home trial judged (D50, amended 24 November 2026): the old home's grade and the new one's by the body's notes, `[old, new]`, kept when the new beats the old by a hundredth of a day; `null` otherwise | per mille of a day of need |
| `people[].copied` | Ways it copied from camp-mates | count |
| `people[].thin_hours` | Hours its fat reserve was under half | person-hours |
| `people[].weak_hours` | Hours it was under a fifth | person-hours |
| `people[].gathered_kj` | What it gathered over the run | kJ |
| `people[].meals` | Meals eaten | count |
| `people[].drinks` | Times it drank | count |
| `people[].thoughts` | Times its brain thought | count |
| `people[].habit.days.*` | Its days by what it had said at the fire (D56; the keys of `years[].decisions.said`) | days |
| `people[].habit.found` | Places it found and told at the fire to one who did not know them | count |
| `people[].habit.bare` | Places it went to and found bare | count |
| `people[].habit.years[].year` | A year of its habit | year |
| `people[].habit.years[].days.*` | Its days that year by what it had said | days |
| `people[].habit.years[].fed_days.*` | Of them, the days it gathered a day of its need or more | days |
| `people[].habit.years[].found.*` | Places it found that year, by what it did the day it found them | count |
| `people[].habit.years[].will` | Its will at the year's end (D50, amended: tempered); `null` if its mind was not read then | 0 to 100 |
| `people[].habit.years[].confidence.*` | Its confidence in each activity at the year's end (D56; `explore`, `gather`, `stay`), the lab's vocation is the highest; `null` when none is held | 0 to 100 |
| `people[].habit.years[].successes.*` | Its days of each activity judged a success that year | days |
| `people[].habit.years[].failures.*` | And judged a failure | days |
| `people[].lines[].day` | A line of its life: its birth, its spouse, its children, their weaning, a lost child, its trials, its death; never a thought, a meal, a drink or a gather | day |
| `people[].lines[].tick` | The tick | tick |
| `people[].lines[].what` | The line, as the chronicle says it | string |
| `records[].id` | A record of the common memory at the end (D55): its number | integer |
| `records[].kind` | `place`, `water`, `trial`, `night`, `reputed` (who is believed to know, D56, PR 1 ter) or `barred` (a place seen, not reached, D53 amended 24 November 2026) | string |
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
| `hearth[].nights[].camp` | The camp: the middle of the place (16 tiles) where the circle gathered (D55, amended 24 November 2026) | `[x, y]` |
| `hearth[].nights[].present` | How many lay down | count |
| `hearth[].nights[].told[].record` | A record told | integer |
| `hearth[].nights[].told[].by` | Its teller | integer |
| `rumours[].day` | A rumour dispelled (D55), the first 1,000 of the run only (the counts in `years[].fire.dispelled` and `found[].heard.dispelled` take them all; a camp of 25 dispels about 27 a day, mostly at a look in passing): the day | day |
| `rumours[].tribe` | The tribe of who went | `"A"` or `"B"` |
| `rumours[].body` | Who went where a teller had said and found nothing | integer |
| `rumours[].place` | The place | `[x, y]` |
| `rumours[].record` | The record it had believed | integer |
| `rumours[].from` | Who had told it | integer |
| `seasons[].tribe` | The exploration's measure (lot 5b, PR 1 ter), one entry per tribe and season of the run (a quarter of the year, counted from the run's start): the tribe | `"A"` or `"B"` |
| `seasons[].season` | The season, from 0 at the run's start (`season % 4`: 0 spring … 3 winter) | integer |
| `seasons[].noons` | Noons of a living body past infancy counted in the season | count |
| `seasons[].bearing_known_x10` | The places a body believes bearing food today (its own knowledge and what it heard), mean over the noons, × 10; `null` without noons | integer |
| `seasons[].lived_x10` | The places its notebook holds (D53, amended 24 November 2026: slept, drank or gathered at, eight at most), mean over the noons, × 10; `null` without noons | integer |
| `seasons[].will_x10` | Its will (0 to 100), mean over the noons, × 10 | integer |
| `seasons[].explore_x10` | Its confidence in exploring (D56, 0 to 100), mean over the noons, × 10 | integer |
| `seasons[].days` | Person-days closed in the season | count |
| `seasons[].looked` | Of them, days gone looking for a new place | count |
| `seasons[].out` | Of them, days out (anything but nothing) | count |
| `seasons[].bare` | Places found bare in the season | count |
| `seasons[].lookers_will_x10` | The will at noon of those who looked, mean over their looked days, × 10; `null` without one | integer |
| `seasons[].lookers_explore_x10` | Their confidence in exploring, likewise | integer |
| `bilan.standing.*.days` | The review's measure (lot 5b: by what way being named or set aside changes what each carries), over the whole run, by the day's standing: `named` (called first by name at the fire), `set_aside`, then by what it said, `joined`, `called`, `alone`, `looked`, `nothing`: person-days | count |
| `bilan.standing.*.why.*` | Of them, why it first went home from gathering: `never` (it never went home from gathering), `full` (it carried what it wanted), `group` (its group went home, itself not full), `nothing_left` (nothing it knew of left to gather, not full), `night` | count |
| `bilan.standing.*.gathered_x10` | What it gathered, in per cent of a day of its need, × 10, on average | integer |
| `bilan.standing.*.at_stop_x10` | What it carried when it first went home, likewise, over the days it went | integer |
| `bilan.standing.*.evening_x10` | What it carried at nightfall, likewise | integer |
| `bilan.standing.*.cut_short_x10` | What it carried when it went home not full (`group` or `nothing_left`), likewise | integer |
| `bilan.groups.stable_permille` | Members of a group two days running: the share of today's mates who were yesterday's, per mille, on average | per mille |
| `bilan.groups.members_two_days` | How many such member-days | count |
| `bilan.groups.same_caller` | Of the followers two days running, those who followed the same caller | count |
| `bilan.groups.followers_two_days` | Followers two days running | count |
| `bilan.couples.*` | At each dawn of conception (life's `conceive_nights` first days of a season), each fertile woman with a living spouse, not with child from before and carrying no infant: `met` (spouse within reach, reserve enough), `apart` (spouse too far only), `thin` (reserve too low only), `apart_and_thin` | count |
| `bilan.confidence.everyone_x10` | The confidence in gathering (D56, 0 to 100) at every noon of a living body past infancy, × 10, on average | integer |
| `bilan.confidence.set_aside_before_x10` | That of the set aside at the noon before the fire that set them aside | integer |
| `bilan.confidence.named_before_x10` | That of the named, likewise | integer |
| `bilan.confidence.set_aside_after` | By day since the day set aside (0: its noon, to 14): `[mean × 10, noons]` | `[[integer, count], …]` |
| `bilan.confidence.named_after` | The same for the named | `[[integer, count], …]` |
| `bilan.who.set_aside.*` | Who was set aside, by band at the noon before: `child` (under 12), `young` (12 to 15), `man`, `woman` (16 to 59), `mother` (with child or carrying an infant), `elder` (60 and over) | count |
| `bilan.who.named.*` | Who was named, likewise | count |
| `bilan.who.noons.*` | Every noon counted, by band: what the others are set against | count |
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
| `move_calls[]` | Each move called at a fire (D56, amended 24 November 2026), in order | list |
| `move_calls[].day`, `.tribe`, `.caller`, `.caller_spouse` | The day (from 1), the caller's tribe, the caller, its spouse then | |
| `move_calls[].camp`, `.to`, `.tiles` | The fire, the new home, the distance between | `[x, y]`, tiles |
| `move_calls[].gain_permille`, `.caller_home_fare_permille` | The gain the caller told, and how it fared at its old home, by its own notes | per mille of a day of need |
| `move_calls[].weighed[]` | Each listener who spoke after it: `body`, `spouse`, `balance_permille` (the gain times `trust`, plus the company it would lack here less there), `trust` (0 to 100), `home_fare_permille` (its own home's grade, `null` if it never slept there), `joined` | |
| `move_calls[].kept_from` | When the caller is a spouse telling a call it kept until its spouse was present: who first called it; `null` otherwise | integer |
| `move_calls[].caller_weighed` | When its spouse said no, how the caller weighed its own going, trusting itself (D56, amended 24 November 2026): `balance_permille`, `joined` (`false`: it stayed with its spouse and keeps the call); `null` otherwise | |
| `move_calls[].unheard[]` | Each present grown who did not weigh it: `body`, `spouse`, `why` (`before`: it spoke before the caller, its day said; `spouse_away`: its spouse was not at the fire, it keeps the call; `trial`: under a trial; `child`: under sixteen) | |
| `goals[]` | Each goal held over several days (D56, amended 24 November 2026): `body`, `tribe`, `day` set, `to`, `tiles`, `load_kj` carried then, `end` (`arrived`, `load`, `hunger`, `thirst`, `dropped`, `null` while held at the end), `end_day` | |
| `map.data_uri` | The page's picture of the map at the run's end, the same PNG: the terrain, the camps red, the living white, the dead black | `"data:image/png;base64,…"` |
| `map.tiles_per_pixel` | The scale: one pixel is so many tiles a side (5 on the vivarium's world) | tiles |
| `map.tiles` | The world's size | `[width, height]` tiles |
| `map.pixels` | The picture's size | `[width, height]` pixels |
| `map.legend.*` | The colours of the dots: `camp`, `alive`, `dead` | `[r, g, b]` |
| `bearers.chunk_tiles` | The food map (lab 6): the side of its cell, one chunk | tiles |
| `bearers.margin_chunks` | The map covers only the chunks a tribe's living stood in at some noon (`presence`) and so many chunks around: the whole world would cost some 45 s a run | chunks |
| `bearers.seasons` | The order of the four counts of a cell | `["spring", "summer", "autumn", "winter"]` |
| `bearers.kinds` | The kinds of plants and trees that bear something, in the order of their first meeting | `["apple", …]` |
| `bearers.cells` | Each covered chunk with bearers, placed as the world places them for the bodies: `cx`, `cy` (a chunk's column and row), the bearers in season in each of the four seasons (a kind is in season in a season when it bears on some day of it, in the world's second year), and the three most numerous kinds with their counts, the most numerous first (`kind` an index into `kinds`) | `[[cx, cy, [sp, su, au, wi], [[kind, count], …]], …]` |
| `relief.cell_tiles` | The relief map (lab 6): one height every so many tiles, twice the map picture's scale | tiles |
| `relief.size` | Its size | `[columns, rows]` |
| `relief.known` | Whether the world has heights (the vivarium's atlas has; a flat test world has not, every height then 0) | `true` or `false` |
| `relief.dm` | The heights at the cells' centres, row by row from the north-west | `[decimetres, …]` |

## Not in the file

- The chronicle (on the full page, `--page full`), the map's picture, the families' pages (worked out from `people`:
  mother, father, spouses, children), the ages table (worked out from `born_day` and `sex`).
- The camp's median position per day: the vivarium works it out once a year (`years[].camp`), not
  per day; the lab can take it from `positions` (the median of a tribe's bodies at noon). The
  presence grid (`presence`) and the positions (`positions`) are the two things sampled for the
  data alone, once a day at noon: where the living of each tribe were, and each body one by one.
