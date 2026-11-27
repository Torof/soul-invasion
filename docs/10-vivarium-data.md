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
- Days count from 1 (the run's first day); ticks from 0. A year (`years[]`, a season's year) is
  the cycle of four seasons, `meta.days_per_year` days; an age (`adult_years`, the ages of the
  people) is counted in age-years of `meta.days_per_age_year` days (D09, amended 24 November
  2026). Today the two are the same, 50 days.
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
| `meta.format` | The format marker, `1` since 22 November 2026, `2` since 26 November 2026 (the ways' `worth_half_tiles` became `worth_half_minutes`) | integer |
| `meta.settings.world` | The world; `"vivarium-wide"` the vivarium's zone twice as wide, `"vivarium-huge"` the same zone 16,384 m a side (24 November 2026) | `"vivarium"`, `"vivarium-wide"`, `"vivarium-huge"` or `"patch"` |
| `meta.settings.side` | A patch's side; `null` on the vivarium's world | tiles |
| `meta.settings.people` | People born at each camp: the whole band with the pyramid start, the grown alone with the founders' | count |
| `meta.settings.start` | How a camp is born (`--start`, the life lot, 25 November 2026): `"pyramid"`, a band of every age shaped on the Hadza and the !Kung (D41, amended), the default since then; `"founders"`, the grown of `adult_years` with `children` and `infants`, as every run before; a run without the key is a founders' run | `"pyramid"` or `"founders"` |
| `meta.settings.children` | The founders' children born at each camp (0 with the pyramid start) | count |
| `meta.settings.infants` | The founders' infants born at each camp (0 with the pyramid start) | count |
| `meta.settings.adult_years` | The founders' grown ages at the start, first to second (unused with the pyramid start) | `[years, years]` |
| `meta.settings.fed` | Food without limit (`--fed`) | boolean |
| `meta.settings.brain` | The people think (false: the control run without a brain) | boolean |
| `meta.settings.tribes` | Tribes, 1 or 2 | count |
| `meta.settings.days` | Game days run | days |
| `meta.settings.ticks_per_second` | The cadence: ticks a game second (the game runs at 10) | ticks |
| `meta.settings.seed` | The run's seed | integer |
| `meta.settings.trials` | The brains try, judge and copy their ways (false: `--no-trials`) | boolean |
| `meta.settings.hands_known` | Grown founders of each tribe who know the deeds of a container from birth, a diagnostic of D57 (`--hands-known`); 0 in the experiment | count |
| `meta.settings.fire` | The evening fire and the common memory, D55 (false: `--no-fire`); since 23 November 2026 | boolean |
| `meta.settings.decisions` | The decisions at the evening fire, D56 (false: `--no-decisions`); since 23 November 2026 | boolean |
| `meta.settings.moves` | Moving home as a consequence (amended 24 November 2026); `false`: the control run, `--no-moves`, nobody is surprised nor moves home | boolean |
| `meta.settings.passages` | The passages remembered and told (D53, amended 26 November 2026); `false`: the control run, `--no-passages`, nobody keeps, tells or walks through a passage | boolean |
| `meta.settings.learning` | Learning causes, believe without acting (D59, 27 November 2026); `false`: the control run, `--no-learning`, the content's `learning` section replaced by none and the problems it marks (`ill_gut`, `ill_chest`) taken out: behaviour bit for bit the run's before the change | boolean |
| `meta.settings.learning_act` | Learning causes, acting (D59, amended 27 November 2026); `false`: `--learning-believe-only`, the content's `learning.acting` replaced by none: beliefs come to, told and taken, no lever, no weight on moves, no trial | boolean |
| `meta.settings.search` | The search for an absent kin (D56, amended 27 November 2026); `false`: the control run, `--no-search`, the content's `search` section replaced by none: behaviour bit for bit the run's before the change | boolean |
| `meta.settings.old_illness` | The control run of illness as episodes (`--old-illness`, D49, amended 26 November 2026): the old rule, a background death by age alone, the content's `illness` section replaced by none; a run without the key is of the old rule when its content had no such section | bool |
| `meta.settings.content` | The content directory given, its last component; `null` when found from the working directory | string |
| `meta.map_tiles` | The world's size | `[width, height]` tiles |
| `meta.camps` | The camps' starting positions, A first | `[[x, y], …]` |
| `meta.days_per_year` | The length of a year: the cycle of four seasons, the nature clock (`--days-per-cycle`) | days |
| `meta.age_years_per_cycle` | Age-years in a cycle, the clock of ages (`--age-years-per-cycle`; D09, amended 24 November 2026); 1 today, the age-year being the cycle | count |
| `meta.days_per_age_year` | The length of an age-year: `days_per_year / age_years_per_cycle` | days |
| `meta.seconds_per_day` | The length of a game day (`--seconds-per-day`) | game seconds |
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
| `meta.timing.land_made` | Chunks of land the run made itself until its end, the picture's apart: tiles, woods, water (D51 amended 25 November 2026) | `[count, count, count]` |
| `meta.timing.land_kept` | Chunks of land read from the file kept (`--no-land-cache`: none): tiles, woods, water | `[count, count, count]` |
| `meta.timing.ground_made` | Chunks of trees and of bearers the run placed until its end, the food map's apart (D51 amended 25 November 2026) | `[count, count]` |
| `meta.timing.ground_kept` | Chunks of trees and of bearers read from the file kept on this world with this content (`--no-land-cache`: none) | `[count, count]` |
| `meta.timing.land_checked` | Chunks of land, then of trees and bearers, the run held before taking the kept files and found the same as theirs: the check that lets a file written by another build be read (D51 amended 25 November 2026) | `[count, count]` |
| `meta.timing.land_prepared_s` | Seconds on the wall the run spent preparing the whole land, then its trees and plants, on all cores, when no whole file was kept; 0 when it read them | `[seconds, seconds]` |
| `meta.timing.learning_s` | Seconds on the wall the brains spent in their learning (D59): the ailments' marks, the episodes and beliefs, the tales of beliefs at dawn; 0 without it | seconds |
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
| `years[].couples.parted` | The tribe's couples parted in the year, apart too long, each counted once (D49, amended 24 November 2026) | count |
| `years[].couples.taken_again` | The spouses its members took in the year after their first, widowed or parted | count |
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
| `years[].ways.median_worth_minutes` | The median of the walk that halves a plant's worth (`median_worth_tiles`, in tiles, until 26 November 2026: the brain in walking time) | game minutes |
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
| `people[].parted[].day` | The day it parted from a spouse, apart too long (D49, amended 24 November 2026) | day |
| `people[].parted[].id` | That spouse | integer |
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
| `people[].ways.worth_half_minutes` | Its ways at the end, or at its death (D50); the whole object is `null` for a mind never read. The walk that halves a plant's worth, in game minutes (`worth_half_tiles`, in tiles, until 26 November 2026) | game minutes |
| `people[].ways.sleep_out` | Whether it sleeps out | boolean |
| `people[].ways.stock_kj` | What it stores before resting | kJ |
| `people[].ways.home` | Its home | `[x, y]` |
| `people[].trials[].day` | A trial's first day | day |
| `people[].trials[].step` | The kind of the step, as `found` names them | string |
| `people[].trials[].words` | The step in words, as the page tells it | string |
| `people[].trials[].judged_day` | The day the world judged it; `null` while under trial, if it died first, or if it ended unjudged | day |
| `people[].trials[].kept` | Whether it was kept; `null` while unjudged | boolean |
| `people[].trials[].untried_day` | A trial of the hands ended unjudged (25 November 2026): the day, its hands having found nothing new to try within reach; `null` otherwise | day |
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
| `records[].kind` | `place`, `water`, `trial`, `night`, `reputed` (who is believed to know, D56, PR 1 ter) `barred` (a place seen, not reached, D53 amended 24 November 2026) or `passage` (the elbows of a way to a place across water or rock, D53 amended 26 November 2026) | string |
| `records[].day` | Its day | day |
| `records[].witness` | Who saw it first, or the trial's body; `null` for a night | integer |
| `records[].refs` | Accesses and nights holding it | count |
| `records[].place` | A place record: the place's key; `null` for another kind, as every key of one kind is | `[x, y]` |
| `records[].states` | Its states, each a kind of food and the bucket of plants bearing it (`place_buckets`) | `[[kind, bucket], …]` |
| `records[].way` | A trial record: the way, `worth_half_minutes` (`worth_half_tiles` until 26 November 2026), `sleep_out`, `stock_kj` or `home` | string |
| `records[].up` | Whether the step went up (wider, out, more; a home moved is up) | boolean |
| `records[].kept` | Whether the trial was kept | boolean |
| `records[].will` | The body's will when it was judged | 0 to 100 |
| `records[].present` | A night record: how many lay down | count |
| `records[].told` | How many records were told | count |
| `hearth[].tribe` | The nights a tribe's hearth still holds at the end (those of its last `hearth_seasons`, D55, D09): the tribe | `"A"` or `"B"` |
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
| `bilan.couples.*` | At each dawn of conception (life's `conceive_nights` first days of a quarter of an age-year, a season while the age-year is the cycle), each fertile woman with a living spouse, not with child from before and carrying no infant: `met` (spouse within reach, reserve enough), `apart` (spouse too far only), `thin` (her reserve leaves her no chance only: weak, since fertility follows the gain, 25 November 2026), `apart_and_thin` | count |
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
| `move_calls[].weighed[]` | Each listener who weighed it, in its last round (D56, amended 25 November 2026: the call weighed in rounds): `body`, `spouse`, `balance_permille` (the gain times `trust`, less the move's cost, plus the company it would lack here less there, plus its family), `trust` (0 to 100), `home_fare_permille` (its own home's grade, `null` if it never slept there), `joined` (its hand up at the end), `worth_permille` (the place's own worth to it: the gain times `trust`, less the move's cost, company and family aside; a hand is raised first only above the margin) | |
| `move_calls[].kept_from` | When the caller is a spouse telling a call it kept until its spouse was present: who first called it; `null` otherwise | integer |
| `move_calls[].caller_weighed` | When its spouse said no, how the caller weighed its own going, trusting itself (D56, amended 24 November 2026): `balance_permille`, `joined` (`false`: it stayed with its spouse and keeps the call); `null` otherwise | |
| `move_calls[].unheard[]` | Each present grown who did not weigh it: `body`, `spouse`, `why` (`before`: it spoke before the caller, its day said; `spouse_away`: its spouse was not at the fire, it keeps the call; `trial`: under a trial; `child`: under sixteen) | |
| `moving.felt[]` | Each move felt at nightfall, then called at the dawn's fire (amended 24 November 2026: a move is a consequence), in order | list |
| `moving.felt[].day`, `.body`, `.tribe` | The day, who, its tribe | |
| `moving.felt[].gain_permille`, `.push_permille`, `.surprise_permille` | The new home's grade less the old one's, season against season; how much worse its recent days were than its usual days of the season; its greatest surprise there | per mille of a day of need |
| `moving.felt[].huge` | `true`: not pushed, the surprise alone was huge enough | boolean |
| `moving.surprised[]` | Each surprise lived: a day gathering at a grove away from home that gave clearly more than the present who stayed near home and gathered that day, in order; or, for a day that ended away from any fire (out on a trip, amended 25 November 2026), more than its own usual days of the season (`stayers` 0, never told) | list |
| `moving.surprised[].day`, `.body`, `.tribe`, `.stayers` | The day it was told, who, its tribe, how many stayers it was compared with (0: none stayed near home that day, it was compared with its own usual days of the season, amended 25 November 2026) | |
| `moving.surprised[].gain_permille`, `.stayers_gathered_permille` | How much more it gathered there; the stayers' mean that day, or its own usual day's when `stayers` is 0 | per mille of a day of need |
| `moving.surprised[].spouse_took_it` | Its spouse at the fire took the day into its own notes | boolean |
| `moving.go_see[]` | Each day gone to see a surprise told at the fire, in order | list |
| `moving.go_see[].day`, `.body`, `.tribe`, `.from` | The day, who, its tribe, the teller | |
| `moving.pushed[]` | By tribe and year, the push felt | list |
| `moving.pushed[].tribe` | The tribe | `"A"` or `"B"` |
| `moving.pushed[].year` | The year | integer |
| `moving.pushed[].dawns` | The grown bodies' dawns that year | count |
| `moving.pushed[].pushed` | Those of them whose push at the last nightfall reached `moving.push_permille` | count |
| `moving.utility[]` | Each move's utility (the lead's measure): the mover against the stayers over the season after | list |
| `moving.utility[].day`, `.body`, `.tribe` | The day it set off, who, its tribe | |
| `moving.utility[].mover_hungry_nights` | Its nightfalls hungry over the season after | count |
| `moving.utility[].stayers`, `.stayers_hungry_nights_milli` | The grown of its tribe alive throughout who moved neither in the season before nor in that one; their mean hungry nightfalls over the same days, in thousandths | |
| `moving.utility[].mover_drink_tiles_before`, `.mover_drink_tiles_after` | The mover's mean tiles from home to its first drink of the day, the days it drank, over the season before it set off and the season after (24 November 2026, the movers' water); null when it never drank there | tiles |
| `moving.utility[].stayers_drink_tiles_after` | The stayers' mean of theirs over the season after; null when none drank | tiles |
| `hands` | The hands (D57, 24 November 2026): the first invention, the container; present in every run, empty without `hands` in the content | object |
| `hands.first` | The first container made in the run: `day`, `age_year_x10` (its age-year × 10, from 0), `body`, `tribe`, `heard_before` (it had heard a deed of the hands told); null without one | object or null |
| `hands.first_listener_made` | The first container made by another body after hearing a deed of the hands told: `day`, `body`, `tribe`, `nights_after_first` (days after the first container); null without one | object or null |
| `hands.made` | Containers made | count |
| `hands.remakes` | Of them, by a body whose own container had torn before | count |
| `hands.makers` | The bodies that made one | count |
| `hands.told[]` | Each telling at a fire of a kept trial of the hands or of a novelty of the hands (D57, 25 November 2026), in order: `day`, `record`, `teller`, `tribe`, `what` (`kept trial`, or the novelty's deed: `pick up flint`, `cut birch with flint`, `fold birch_bark`), `heard` (who believed it that night), `missed` (its tribe's living past infancy not at that fire) | list |
| `hands.heard` | Bodies that heard a deed of the hands told | count |
| `hands.heard_never_tried` | Of them, those that never began a trial of the deeds heard | count |
| `hands.heard_across_tribes` | Hearings by a listener of the other tribe than its teller (0 while camps do not visit) | count |
| `hands.gestures.*` | By gesture (`cut`, `fold`): `done` (ended), `gave` (something came of it); a pick-up is D35's act and not counted here | `{done, gave}` |
| `hands.known.*` | By deed (`pick up flint`, `cut birch with flint`, `fold birch_bark`, ...): the living past infancy who know it at the run's end | count |
| `hands.cuts_with.*` | By the material of the tool (`flint`, `pebble`, `branch`, ...): the cuts `done` and those that `gave` something | `{done, gave}` |
| `hands.barks_dropped` | Cuts whose bark fell at the cutter's feet, its load too full to take it (D57, amended 25 November 2026: a body no longer cuts what it cannot carry, so only a load filled during the gesture counts here) | count |
| `hands.tribes[]` | One per tribe | list |
| `hands.tribes[].tribe` | Its name | `"A"` or `"B"` |
| `hands.tribes[].first` | Its first container, as `hands.first`: the second tribe finding it alone, or not | object or null |
| `hands.tribes[].makers` | Its bodies that made one | count |
| `hands.tribes[].told` | Tellings by its tellers | count |
| `hands.tribes[].heard_from_tribe` | Listeners of a teller of this tribe | count |
| `hands.tribes[].years[]` | By age-year from 1 | list |
| `hands.tribes[].years[].age_year` | The age-year | from 1 |
| `hands.tribes[].years[].living` | Living past infancy at its end | count |
| `hands.tribes[].years[].carriers` | Of them, carrying a container | count |
| `hands.tribes[].years[].served_carriers` | Of the living, carrying a thing that once served them (a flint whose cut gave something, a container that held food beyond the arms), kept by their memory of use (D57, 25 November 2026) | count |
| `hands.tribes[].years[].made` | Containers made | count |
| `hands.tribes[].years[].remade` | Of them, remakes after a tear | count |
| `hands.tribes[].years[].torn` | Containers torn | count |
| `hands.tribes[].years[].trials` | Trials of the hands begun | count |
| `hands.tribes[].years[].trials_heard` | Of them, on a tale heard | count |
| `hands.tribes[].years[].kept` | Trials of the hands kept | count |
| `hands.tribes[].years[].untried` | Trials of the hands ended unjudged, no gesture new to the hands having been begun in their days, nothing new being within reach (D57, amended 25 November 2026): no lesson told, no will moved | count |
| `hands.tribes[].years[].drew_free` | Trials of the hands begun on no tale that drew a gesture at random | count |
| `hands.tribes[].years[].drew_heard` | Trials of the hands begun on a tale that drew a gesture at random, no gesture heard being possible there (D57, amended 25 November 2026); `drew_free + drew_heard` are the trials that truly tried something new | count |
| `hands.tribes[].years[].tools_taken` | Tools taken up for a cut known or heard, when none was carried (D57, amended 25 November 2026: "cut a birch with a flint" holds the flint) | count |
| `hands.tribes[].years[].home_with_kj` | The food carried at nightfall by the grown at home (within `HOME_TILES` of it) with a container, a night's mean; null without such a night | kJ |
| `hands.tribes[].years[].home_with_nights` | The nights counted | count |
| `hands.tribes[].years[].home_without_kj` | The same without a container | kJ |
| `hands.tribes[].years[].home_without_nights` | The nights counted | count |
| `hands.tribes[].years[].noon_tiles_x10` | The grown's distance from home at noon, on each axis the larger, mean × 10 | tiles × 10 |
| `hands.tribes[].years[].home_changes` | Homes changed in the age-year (`home_changes`) | count |
| `settlements[]` | The settlements at each year's end and the run's (24 November 2026): the living whose homes lie within 64 tiles (`SETTLEMENT_TILES`, its own measure since 24 November 2026, no longer `mate_within_tiles`) of one another on each axis, link by link; an infant counted at its carrier's home, a body without a mind where it stands | list |
| `settlements[].year` | The year | integer |
| `settlements[].groups[]` | Its settlements, the largest first | list |
| `settlements[].groups[].at` | The mean of its homes | `[x, y]` tiles |
| `settlements[].groups[].people` | The living there | count |
| `settlements[].groups[].by_tribe` | Of them, the first and the second tribe's | `[count, count]` |
| `partings[]` | Each parting (24 November 2026, why couples part), with the couple's dawns before it; distances in tiles on the longer axis, as the parting rule counts | list |
| `partings[].day` | The day it parted | day |
| `partings[].pair` | The two, the lower id first | `[id, id]` |
| `partings[].dawns[]` | Their dawns as spouses over the 12 days before, up to the parting's own, the oldest first | list |
| `partings[].dawns[].day` | The day | day |
| `partings[].dawns[].apart` | The spouses apart where they woke | tiles |
| `partings[].dawns[].from_home` | Each one from its own home, the lower id first | `[tiles, tiles]` |
| `partings[].dawns[].homes_apart` | Their two homes apart | tiles |
| `home_changes[]` | Each grown body's home changed between two dawns, whatever moved it (24 November 2026, why daughter villages), against the settlements of the dawn before: the others' homes linked within `SETTLEMENT_TILES` (64) on each axis, link by link; a village is such a settlement of 10 or more that does not reach the old home | list |
| `home_changes[].day` | The dawn it was seen | day |
| `home_changes[].body` | Who | id |
| `home_changes[].tribe` | Its tribe | 0 or 1 |
| `home_changes[].from` | The home before | `[x, y]` tiles |
| `home_changes[].to` | The home after | `[x, y]` tiles |
| `home_changes[].with` | The other grown bodies whose home changed the same dawn to within reach of this new home | count |
| `home_changes[].left_people` | The people of the settlements the old home reached, the mover out | count |
| `home_changes[].joined_people` | The people of the settlements the new home reaches, the mover out | count |
| `home_changes[].villages` | The villages there were | count |
| `home_changes[].knew_villages` | Of them, those the mover knew a place within reach of | count |
| `home_changes[].nearest_village` | The nearest village to the new home, `{tiles, people}`; null without any | object or null |
| `home_changes[].known_places` | The places the mover knew of | count |
| `nights_apart[]` | The fertile couples' dawns, by age-year from the first (24 November 2026, the project lead: births need couples together at night): every dawn of a fertile wife not with child whose spouse lives; apart is farther than `mate_within_tiles` on either axis. Each dawn apart has one cause, the first that holds for a spouse away from its home | list, one per year |
| `nights_apart[].dawns` | The fertile couples' dawns | dawns |
| `nights_apart[].together` | Those spent together | dawns |
| `nights_apart[].homes_apart` | Apart: their homes farther apart than `mate_within_tiles` | dawns |
| `nights_apart[].sleep_out` | Apart: one away from home sleeps out by its way (D50) | dawns |
| `nights_apart[].goal` | Apart: one away from home holds a goal beyond a day (D56) | dawns |
| `nights_apart[].home_trial` | Apart: one away from home is on its home trial | dawns |
| `nights_apart[].caught_out` | Apart: none of these; the night caught one out | dawns |
| `nights_out[]` | The grown bodies' dawns, by age-year from the first (25 November 2026, the project lead: do small groups ever spend nights out together?); away is farther from its home than the fire's reach (`trials.imitate_tiles`) on either axis | list, one per year |
| `nights_out[].dawns` | The grown bodies' dawns | dawns |
| `nights_out[].away` | Those away from home | dawns |
| `nights_out[].asleep` | Of them, asleep | dawns |
| `nights_out[].in_company` | Of them, with two or more other grown bodies away from their own homes lying within the fire's reach: a group's night out | dawns |
| `pooling.on` | Food pooled at the hearth (D58, 25 November 2026), its own block: whether food was pooled (the content's `pooling` section on and the fire lit; `--no-pooling` turns it off) | `true` or `false` |
| `pooling.care` | Whether the care for the young's coming days was on (`--no-care` turns it off) | `true` or `false` |
| `pooling.fed_until_years` | The age the hearth feeds a child until | years |
| `pooling.totals.*` | Over the run, all hearths: `put_kj` put down at a reserve, `eaten_kj` eaten from one, `eaten_young_kj` of it eaten by the young (under sixteen), `spoiled_kj` spoiled there | kJ |
| `pooling.hunger_bar.*.*` | The hunger bar (D58, amended 25 November 2026), over the run: the meals of the hearth's food a hungry body ate because its hunger gave way, `on_the_way` (food it carried for its hearth, eaten away from the reserve) and `despite_the_young` (from the reserve, when the young of the hearth would otherwise have kept it), each split `very_hungry` (fat left under `very_hungry_fat_percent`, its will not keeping it that game day) and `starving` (under `starving_fat_percent`). Counted when the body decides the meal, one an eating gesture; all zero without the content's bar | meals |
| `pooling.children.*` | `young` and `under_eight`, the bodies under sixteen and under eight at some point of the run (born in it, or that young at the start); `dead_before_sixteen` and `dead_before_eight`, of them dead before that age; `born`, born in the run | bodies |
| `pooling.parents.*` | The grown (sixteen and more) at each nightfall who are parents of a living child under eight: `body_days`; `out`, the body-days it went farther from its home than `HOME_TILES` (6) on either axis at some step (to drink too); `gathered`, the body-days it gathered something; `tiles`, tiles walked in all, `tiles_per_day` a body-day; `fatigue_at_nightfall`, the mean fatigue then; `thin`, `weak`, the body-days thin or weak at nightfall | body-days, tiles, points |
| `pooling.others.*` | The same for the other grown | body-days, tiles, points |
| `pooling.days[].*` | Day by day (from 1): `day`; `reserve_dawn_kj` (all reserves at dawn), `put_kj`, `eaten_kj`, `spoiled_kj`; `grown` and `out` (the grown at nightfall, and those that went out that day), `out_percent`; `gathered` and `gathered_percent` (those that gathered something that day: going out to drink is not working); `gathering_for` (the bodies that set out at dawn to gather for their hearth beyond their own store), `seen_kj` and `cared_kj` (what for: their own seen hungry, their care, before the greater is taken) | kJ, bodies, % |
| `pooling.days[].hearths[].*` | Each hearth that day: `camp` (`[x, y]`, its place's middle), `dawn_kj`, `put_kj`, `eaten_kj`, `eaten_young_kj`, `spoiled_kj` | kJ |
| `pooling.days[].eaters.*` | Who ate at the hearths that day (the food carried home, 25 November 2026): `under_eight_kj` (children under eight, fed by the hearth), `eight_to_sixteen_kj`, `parents_kj` (the grown parents of a living child under eight, read at the day's first tick), `others_kj` (the other grown) | kJ |
| `pooling.eaters.*` | The same over the run | kJ |
| `pooling.hunger_deaths[].*` | Every death of hunger (measures only, 25 November 2026), read at the body's last living tick, its vitality under a point (it thinks no more, eats no more, carries the same until it dies): `day`, `body` (its id), `tribe`, `at` (`[x, y]`, where it died); `carried_kj`, the food it carried, not spoiled; `hearth_kj`, of it what it carried for its hearth (D58, not its own to eat), `own_kj` the rest; `camp_tiles`, from where it died to its hearth's camp, on the longer axis; `ticks_since_meal` and `hours_since_meal` (game ticks and whole game hours, a 24th of the game day, since it last ate anything, `null` if it never ate in the run); `age_years` (whole age-years), `will` (0 to 100), `parent_under_8` (a parent of a living child under eight); `reserve_kj`, its hearth's reserve then (0: empty; `null` without pooling); `hunger`, its hunger gauge (0 to 100). Every field but `day`, `body`, `tribe`, `at` and the meal is `null` when its vitality emptied in one tick (no last living tick seen) | kJ, tiles, ticks, game days, age-years, points |
| `pooling.arms.parents.*` | The gathers of the grown parents of a living child under eight (read at the day's first tick), ended over the run: `gathers` (every gather ended); `full`, those ended because the body could carry no more; of them `full_arms` (its arms alone full: no container, its loose food at `movement.hands_g` by its size, D57), `full_containers` (arms and containers full), `full_load` (its whole load full, `carry_capacity_g`); `left_kj`, what the plant still held of that crop when a gather ended full (left there, for later or for others); `left_unknown`, full gathers whose plant could not be read (begun and ended in one tick); `gathered_kj`, what the full gathers had gathered | gathers, kJ |
| `pooling.arms.others.*` | The same for the other grown | gathers, kJ |
| `pooling.arms.young.*` | The same for the young (under sixteen) | gathers, kJ |
| `pooling.arms.hunger_at_full_arms.*` | The hunger gauge at each gather ended with the arms alone full: `tenths`, eleven counts by tenth of the gauge, the first for 0–9 points, the last for 100 (full); `full_by_reserve`, of those at 100, four counts by the fat reserve left (0–24 %, 25–49 %, 50–74 %, 75–100 %): the gauge full only says the fat is burning, the reserve says how near starving | gathers |
| `pooling.intake.parents.*` | The grown parents of a living child under eight (at the day's first tick), each body's season by class (a body changing class in a season counts in both): `adult_seasons`, `body_days` (nightfalls lived), `eaten_kj` (everything it ate, from nightfall to nightfall), `hearth_kj` (of it, from its hearth's reserve), `need_kj` (its day's burn at each nightfall, walking not counted: some 6 % more), `eaten_percent` (eaten against need, all its days); then the adult-seasons by their own eaten against need: `under_half`, `half_to_three_quarters`, `three_quarters_to_full`, `full_or_more` | kJ, body-days, %, adult-seasons |
| `pooling.intake.others.*` | The same for the other grown | kJ, body-days, %, adult-seasons |
| `pooling.intake.seasons[].*` | The same season by season: `cycle` (from 1), `season`, then `parents` and `others` | cycles |
| `pooling.intake.seasons[].parents.*` | That season's parents, as `pooling.intake.parents` | as above |
| `pooling.intake.seasons[].others.*` | That season's other grown, as `pooling.intake.others` | as above |
| `pooling.carrying.*.*` | Hunger while carrying food for the hearth (25 November 2026): histograms of the hunger gauge, each `tenths` and `full_by_reserve` as `pooling.arms.hunger_at_full_arms` (the brain's `HUNGRY` is 50, the sixth tenth). Sampled at **every thought** of a body holding food for its hearth (what it carries for its hearth, or put down at that thought, no more than the food it carries): `ate_own`, it chose to eat food it carries; `ate_hearth`, to eat at its hearth's reserve; `ate_lying`, food lying about; then, hungry (gauge at 50 or more) and not eating: `hungry_putting_down`, it put food down at its hearth; `hungry_blocked`, anything else, all it carries being its hearth's (nothing of its own to eat, D58); `hungry_own_left`, anything else with food of its own beyond it. And **once a game hour**, every body holding food for its hearth: `hours_blocked` (all it carries its hearth's), `hours_own_left` (food of its own beyond it). A thought is the brain's: a body walking home thinks seldom (D53), so the hourly counts weigh the time held | thoughts, body-hours |
| `conception.gain_days` | The conceptions (measures only, 25 November 2026), their own block: the content's `conceive_gain_days`, the dawns over which a woman's reserve gain is read (D49, amended 25 November 2026: fertility follows the gain; until that day `gate_percent`, the reserve gate) | dawns |
| `conception.by_gain.*.*` | The checks that reached the draw (every condition met) by the woman's reserve gain (`Body::reserve_gain_percent`, the mean a day over the last `gain_days` dawns, in per cent of her day's need): `falling` (under 0), `slow` (0 to under the content's last point), `good` (at it or over, or her reserve at `conceive_full_percent`); each with `met` (the checks) and `conceived` (those that conceived) | checks |
| `conception.checks.*` | Every check of a fertile woman (16 to 42) at a dawn the world's conception rule runs (the quarter's first `conceive_nights` days), by the first condition that barred her, in the rule's order: `pregnant` (with child already); `weak` (under the content's `weak_percent` of her reserve: no chance; until 25 November 2026 `reserve_0_25`, `reserve_25_50`, `reserve_50_80`, under the old gate); `infant` (an infant in her arms); `never_wed`, `widowed`, `parted` (no spouse: never wed; her last spouse dead; parted from her last spouse, D49 amended); `spouse_far` (her spouse beyond `mate_within_tiles`); `draw_no` (every condition met, the quarter's draw said no; also one who died in that very tick); `conceived`. Read on the world just before the tick that runs the rule (what runs before it in that tick touches no body), whether she conceived read just after it from her pregnancy | checks |
| `conception.woman_quarters.*` | The same by woman-quarter (a woman's quarter of an age-year, checked on several dawns with one draw, so the checks over-count): `conceived` when she conceived on one check; else the condition that barred every check of the quarter, under the same keys; `mixed` otherwise; `total`, the woman-quarters checked | woman-quarters |
| `conception.lost[]` | Every pregnancy lost (D49: she grew weak), in order: `day` (from 1), `mother` (the body's id), `reserve_percent` (her reserve left then, in per cent of the whole, read at the loss's tick) | day, id, % |
| `conception.births` | Children born in the run | count |
| `conception.birth_days` | Their days, in order | days from 1 |
| `conception.seasons[].*` | Season by season of the run: `cycle` (from 1), `season`; at every dawn, `fertile_dawns` (the fertile women), `rising` (of them, their reserve gain over 0; `above_gate` until 25 November 2026), `free_dawns` (the fertile women neither with child nor with an infant in her arms) and `free_rising` (of them, rising) | woman-dawns |
| `meetings.hour_ticks` | The meetings between tribes (measures only, 25 November 2026; the project lead doubted the tribes never meet, read at noon alone), their own block: the world is read at the first tick of every game hour, every this many ticks (a day's ticks over 24) | ticks |
| `meetings.hours` | Game hours read | hours |
| `meetings.hours_in_sight` | Hours with at least one pair in sight: two living bodies of different tribes, neither carried, within the sight of one of them (the sim's own, `World::sight_tiles_at`, D32: by the ground each stands on, 60 tiles on open ground, 25 in the heart of a wood; tiles on the longer axis, a tile a metre) | hours |
| `meetings.pair_hours` | Pairs in sight, summed over the hours | pair-hours |
| `meetings.pairs` | Distinct pairs of bodies ever in sight | pairs |
| `meetings.first_day` | The day of the first hour with a pair in sight; `null` when none | day from 1 |
| `meetings.first_hour` | Its game hour, 0 at dawn to 23; `null` when none | hour |
| `meetings.seen[].*` | The first 12 hours with a pair in sight, in order: `day`, `hour`, the nearest pair in sight that hour, `a` and `b` (body ids, the lower first) with `a_at` and `b_at` (`[x, y]` tiles), `tiles` between them (longer axis), `pairs` in sight that hour | day, hour, id, tiles, pairs |
| `meetings.seasons[].*` | Season by season of the run: `cycle` (from 1), `season`, `hours` read, `hours_in_sight`; the nearest two bodies of different tribes over its hours, in sight or not: `nearest_tiles`, `nearest_day`, `nearest_hour`, `nearest_at` (two `{body, at}`, `at` as `[x, y]` tiles); `null` with fewer than two tribes alive. The cost: the living sorted by x once an hour, then a sweep comparing a body only with those within the larger of the widest sight and the nearest found so far on x (every pair at worst, 1,770 comparisons an hour for sixty bodies), and one look at the ground under each body: measured at 6 µs an hour for sixty bodies, 0.06 s over 400 journées | cycles, hours, tiles, day, hour, id |
| `meetings.seasons[].nearest_at[].*` | The season's nearest two: `body` (id) and `at` (`[x, y]` tiles) | id, tiles |
| `reach[]` | How far the living went, year by year (24 November 2026, the brain in walking time: "Faut le passer non en mètres, mais en temps de marche"). A walk is in game minutes (a 1,440th of the game day) of each one's own walk, at the pace it felt at its last nightfall on the ground it lay on (the reference ground's before any), straight from its home; tiles beside | list |
| `reach[].year` | The year | integer |
| `reach[].gathers` | The gathers ended that year | count |
| `reach[].gather_minutes_x10_median` | Where each gather ended, from the gatherer's home: the median | tenths of a game minute |
| `reach[].gather_minutes_x10_p90` | The same, the 90th centile | tenths of a game minute |
| `reach[].gather_tiles_median` | The median in tiles | tiles |
| `reach[].gather_tiles_p90` | The 90th centile in tiles | tiles |
| `reach[].notebook_places_mean` | The places in the notebook of each grown one at each dawn (D53), on average, rounded down | count |
| `reach[].notebook_farthest_minutes_x10_median` | The notebook's farthest place from home, at each dawn of each grown one: the median | tenths of a game minute |
| `reach[].notebook_farthest_minutes_x10_max` | The same, the most | tenths of a game minute |
| `reach[].notebook_farthest_tiles_median` | The median in tiles | tiles |
| `reach[].notebook_farthest_tiles_max` | The most in tiles | tiles |
| `reach[].forgotten` | The places the notebooks forgot for want of room (`thought.places_remembered`) | count |
| `reach[].forgotten_tiles_mean` | How far from home they lay, on average, rounded down | tiles |
| `problems.on` | The problems (D50 and D57, amended 26 November 2026), their own block, last: whether the content remembers any (`content/needs.ron`, section `problems`); `false` in the control run `--no-problems` | bool |
| `problems.person_days` | The living counted once a day, summed: the person-days the rates below are taken over | person-days |
| `problems.kinds[].*` | Each kind of the content, in its order: `refusal` (today `full`: a gathering stopped with the arms full, the plant still bearing, no container holding anything) and `property` (the one its test reads, `holds_g`); `lived` (times a body lived it), `lived_per_1000_person_days`, `lived_hungry` (of them, lived at the brain's `HUNGRY`, 50, or more); empty when off | count, per 1,000 person-days |
| `problems.kinds[].trials.*` | The trials of the hands begun from it: `waited` (times the will made a body mean to try it and it could not begin there and then: not free, or nothing new to gesture around; the problem waits on its list), `began`, `began_on_the_spot` (there and then, where it was lived), `began_later` (at a later free moment, a calm thought by day or nightfall, from the top of its things to think about), `began_on_a_tale` (of them, aimed first at deeds told at the fire with the same problem), `kept` (a later gathering stopped full found the body carrying more than its arms alone, and its hands learnt something), `abandoned` (`gives_up_seasons` passed with the problem lived again unmoved), `untried` (ended unjudged: nothing new tried), `lapsed` (ended unjudged: its time out, the problem never back). Each is also a trial of the hands in `hands` and in the trial lines | trials |
| `problems.first_container_day` | The day the first container was made (the same as `hands.first.day`); `null` when none | day from 1 |
| `problems.drew_free` | Gestures drawn at random in trials of the hands begun on no tale, over the run (the sum of `hands.tribes[].years[].drew_free`) | trials |
| `problems.drew_heard` | The same in trials begun on a tale (the sum of `drew_heard`) | trials |
| `problems.cycles[].*` | At each cycle's last tick, and the run's: `cycle` (from 1), `living` (infants aside), `carriers` (of them, carrying a container), `mean_will` (their mean will, 0 to 100, rounded to the point) | cycle, bodies, will |
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
| `land_food.ring_tiles` | The land's food (25 November 2026, measures only): what the trees and plants hold, day by day, had nobody picked any (D45, D48), worked out once after the run on the chunks of the food map (`bearers`) and every chunk within the wider ring of a tribe's camp at some noon. The two rings around a camp, on the longer axis | `[500, 1000]` tiles |
| `land_food.chunks` | The chunks measured (a chunk is 32 × 32 tiles, about 0.001 km²) | count |
| `land_food.crops` | The crops, in the order of `content/crops.ron`: `"kind/material"` | `["hazel/hazelnuts", …]` |
| `land_food.kj_per_kg` | Each crop's material's energy (`materials.ron`), in the same order | kJ a kg |
| `land_food.plants` | Each crop's trees or plants on the land measured | count |
| `land_food.bears_g` | Each crop's grams on the land measured, before any picking, crop-year by crop-year from the first (a crop running into the next year counts in the year it began): its kind's lean, usual or rich year around each plant times the plant's share (`World::bears_of_crop`) | `[[grams, …], …]` |
| `land_food.days` | Each noon of the run: the day (from 1), the kilojoules on the plants of the whole land measured then (each crop's `bears_g` of the year on its plant times its curve, `ripen`), and per tribe, in the order of the camps, its camp that noon (the median home of its living past infancy) with the kilojoules within each ring of it, or `null` when the tribe has no living. Untouched: picks are stored by the crop's year, so a past day's leftovers cannot be read after the run | `[[day, kJ, [[[x, y], [kJ, kJ]] or null, …]], …]` |
| `land_food.picked_g` | What each tribe picked, season by season of the run (the `WorkStopped` events' grams): the season from 0 (the first spring; 4 is the second spring), the tribe (0 or 1), and each crop's grams in the order of `crops` | `[[season, tribe, [grams, …]], …]` |
| `land_food.camps[].at` | Each camp a tribe had at some noon, in the order first met | `[x, y]` |
| `land_food.camps[].bears_g` | Each crop's grams within each ring of that camp, before any picking, crop-year by crop-year: what a change of the crops' seasons or curves would put near the camp, worked out without a run | `[[[grams, …], [grams, …]], …]` per crop, rings in the order of `ring_tiles` |
| `relief.cell_tiles` | The relief map (lab 6): one height every so many tiles, twice the map picture's scale | tiles |
| `relief.size` | Its size | `[columns, rows]` |
| `relief.known` | Whether the world has heights (the vivarium's atlas has; a flat test world has not, every height then 0) | `true` or `false` |
| `relief.dm` | The heights at the cells' centres, row by row from the north-west | `[decimetres, …]` |
| `chain.ticks_per_hour` | La chaîne du manque (24 November 2026, the project lead: "Déménager, l'épuisement, aller loin très important… explorer très important"): the measures the rest of the data did not hold, kept by `server/vivarium/src/chain.rs`, a probe that reads the world and the minds and changes nothing. A game hour: a twenty-fourth of a game day | ticks |
| `chain.years[].tribe` | A tribe's year of the chain (the season's year, as `years[]`): the tribe | `"A"` or `"B"` |
| `chain.years[].year` | The year, from 1 | integer |
| `chain.years[].lived_hours` | The living (vitality above 0), counted once a game hour, as the curves' samples count them | person-hours |
| `chain.years[].walker_hours` | Of them, those past infancy: who walk on their own | person-hours |
| `chain.years[].need_kj` | What the living burn: a twenty-fourth of each one's day's need each game hour (a child less, a nursing woman more, an infant nothing); `years[].food.eaten_kj` is set against it, with `walk_kj` | kJ |
| `chain.years[].walk_kj` | What the walkers' walking cost on top of the day's need, as the world charges it (`walk_j_per_m`, less for a smaller body) | kJ |
| `chain.years[].walked_m` | The walkers' walking: the straight stretch between their positions tick after tick, summed | metres |
| `chain.years[].gathering_ticks` | Ticks a walker spent at a work: gathering at a plant or tree (D45); `/ ticks_per_hour` is game hours | ticks |
| `chain.away[]` | Why a grown body away from home at dawn is there, by age-year from the first, in the order of `nights_out` (whose `away` its keys share out): by how it lay down last, asleep still or since the dawn before (the rested wake with the day, at the dawn's own tick, so `nights_out[].asleep` misses them) | list, one per year |
| `chain.away[].exhausted` | Collapsed of fatigue: the world laid it down where it stood, its fatigue full | dawns |
| `chain.away[].stopped` | Laid down by its brain away from home, neither by its way nor for a goal: stopped short, no way home left (water, rock, no way seen) | dawns |
| `chain.away[].stopped_by.*` | Of them, what barred its last walk since dawn (`Event::BodyBlocked` or `MoveRefused`): `water`, `rock`, `no_way`, or `none` told | dawns |
| `chain.away[].sleep_out` | Laid down where it stood, its way being to sleep out (D50): by choice | dawns |
| `chain.away[].goal` | Laid down where it stood, holding a goal beyond a day (D56): by choice | dawns |
| `chain.away[].trip` | Laid down where it stood, out on a trip it chose at its point of no return or on a morning away (D53, D56, amended 25 November 2026): by choice; counted before `sleep_out` | dawns |
| `chain.away[].lay_at_home` | Laid down within the fire's reach of its home: its home moved away from it since | dawns |
| `chain.away[].awake` | Awake at dawn and never lay down since the dawn before: it walked or stood the night through | dawns |
| `chain.away[].woke` | Awake at dawn, having lain down since the dawn before and risen in the night (to drink, or called away): on its feet away from home; whoever rose with the day (the rested rise at the dawn's own tick) is told by how it lay down | dawns |
| `chain.away[].unseen` | Asleep, its lying down not seen | dawns |
| `chain.away[].fatigue_mean` | The fatigue at lying down of those who lay down away (all but `awake`, `woke` and `unseen`), on average; `null` without one | points of 100 |
| `chain.looks` | Every looking day (`Act::Looked`, D56), in order: the day it closed (its nightfall), the tribe (0 or 1), and the farthest it stood from its home that day, on the longer axis, looked at once a game minute | `[[day, tribe, tiles], …]` |
| `chain.outings` | Every outing of a grown body beyond the fire's reach of its home (`trials.imitate_tiles`), as it ended, in order (25 November 2026): the day it woke home again, the tribe (0 or 1), the nights it woke away (0: out and back by the next dawn), and the farthest it stood from its home, on the longer axis, looked at once a game minute; then who went, as it was when the outing began (the first game minute it stood beyond the fire's reach): its will (0 to 100), whether it was a parent of a living child under 8 age-years, of one under 16, its sex (`"female"` or `"male"`), and its age in whole age-years. An outing still open at the end, or ended by death, is not there | `[[day, tribe, nights, tiles, will, parent_under_8, parent_under_16, sex, age_years], …]` |
| `chain.turns.no_return.*` | What the bodies chose away from home when the straight walk home took the rest of the day's light (`News::Turned`, D53, D56, amended 25 November 2026): `home`, `stay` (food in sight, it stays out) or `on` (it goes on with its trip) | choices |
| `chain.turns.morning.*` | The same, on a morning they woke on a trip | choices |
| `passages.dawns_stopped_by_water` | The passages (D53, amended 26 November 2026; measures counted the same with `--no-passages`, where only this key, `marks.made`, `marks.lifted_stood` and `beyond_reached` can be other than 0), their own block, last: the body-days on which a walk was barred by water (`Event::BodyBlocked` or `MoveRefused` with `Blockage::Water`), once a body a day | body-days |
| `passages.marks.made` | Marks of seen, not reached made by a body's own barred walk (`News::Barred` with a cause; the same as the sum of `years[].decisions.barred.made`) | marks |
| `passages.marks.lifted_stood` | Marks lifted by the body standing on the place (`News::Reached`) | marks |
| `passages.marks.lifted_heard` | Marks lifted by hearing a passage to the place at the fire: the way now known | marks |
| `passages.found` | Passages found: a body stood on a place of its notebook that a straight walk from its camp could not reach (water or rock in the way) and kept the elbows of the way it came | passages |
| `passages.told` | Passages told at a fire, a telling each (its new ones, then those of the places it told that night; `passages.told_per_night` at most) | tellings |
| `passages.heard` | Passages heard and held: a present who knew the place and held no passage to it | listeners |
| `passages.walks.begun` | Walks to a place begun through a passage: the place not on a clear line from where the body stood | walks |
| `passages.walks.arrived` | Of them, those that stood on the place at the end | walks |
| `passages.walks.dropped` | Passages dropped, a walk there barred by water or rock (the place then marked seen, not reached) | passages |
| `passages.beyond_reached` | Places of their notebooks the bodies stood on, the first time a day, that a straight walk from their camp could not reach, water or rock in the way, whether through a passage or not | body-place-days |
| `passages.dropped_heard` | Of `walks.dropped`, the passages heard from another, not found | passages |
| `passages.beyond_places` | The distinct places of `beyond_reached` | places |
| `passages.gathered_kj` | What the living gathered, the gathers' kilojoules (`Event::WorkStopped`) | kJ |
| `passages.gathered_kj_beyond` | Of it, what was gathered where the straight line from the gatherer's home crosses water or rock (`sim::map::line_clear`, the brain's clear line) | kJ |
| `passages.camps[].*` | The same by home (`Mind::home`, the camp), in order of the home: `home` (`[x, y]`), `beyond_reached`, `beyond_places`, `gathers`, `gathers_beyond`, `gathered_kj`, `gathered_kj_beyond`; the land around a camp decides most of the beyond (the audit of 26 November 2026) | tiles, counts, kJ |
| `passages.farthest_through_minutes_x10` | The farthest place reached through a passage: the straight walk from the walker's home to where it stood, at its own pace (`Mind::walk_milliminutes`); 0 when none | tenths of a game minute |
| `illness.on` | Illness as episodes (D49, amended 26 November 2026), its own block, last: whether the content holds an `illness` section (`content/needs.ron`); `false`, and no other key, under the old rule (the section absent, or the control run `--old-illness`) | bool |
| `illness.body_days` | The bodies counted at each dawn, summed: the body-days the sick days are taken over | body-days |
| `illness.sick_body_days.*` | Of them, ill at dawn, by ailment: `gut`, `chest` | body-days |
| `illness.episodes_all` | Episodes begun over the run (`Event::FellIll`) | episodes |
| `illness.episodes_caught` | Of them, those whose draw fell on the share the hearth's sick added (contagion) | episodes |
| `illness.episodes[].*` | Episodes begun by `ailment` (`gut` or `chest`), `season` of the day it began (`spring` … `winter`) and `age` band (`0-1`, `2-4`, `5-14`, `15-39`, `40-59`, `60+`, in age-years): `episodes`, and of them `caught` | episodes |
| `illness.ends[].*` | Episodes ended (`Event::IllnessEnded`) by `ailment`, `age` band and `thinness` at the end (`fat`: at least `thin_percent` of its reserve left; `thin`: under it; `weak`: under `weak_percent`): `healed`, `died`. A body dead of something else while ill is in neither | episodes |
| `illness.deaths_columns` | The columns of `illness.deaths`: `day`, `ailment`, `age`, `thinness`, `reserve_percent`, `hearth_members`, `hearth_fouling` | names |
| `illness.deaths` | Every death of illness, in order: the day (from 1), the ailment, the age band, the thinness band, the reserve left (percent), its hearth's members and fouling (body-nights) the dawn of the day it fell ill | rows |
| `illness.hearth_places` | The distinct hearth places of `illness.hearths` | places |
| `illness.hearths_columns` | The columns of `illness.hearths`: `day`, `x`, `y`, `members`, `sick_gut`, `sick_chest`, `fouling` | names |
| `illness.hearths` | Camp fouling over time: at each dawn, each hearth place with two members or more (a hearth is the square of `thought.place_tiles` its members' homes lie in, as the brains told them; a body without a home stands for its own), the day, the place's column and row (in places, not tiles), its members, its sick of the gut and of the chest at dawn, and its fouling (the body-nights slept in the place, fading `fouling_keep_permille` a dawn, whole body-nights) | rows |
| `illness.moves_all` | Camps moved: at a dawn, the homes of some bodies told in another place than the dawn before, counted once by (place left, place reached) | moves |
| `illness.moves_with_sick` | Of them, those whose place left had sick members at the dawn before | moves |
| `illness.moves_columns` | The columns of `illness.moves`: `day`, `from_x`, `from_y`, `to_x`, `to_y`, `movers`, `members_before`, `sick_gut_before`, `sick_chest_before`, `fouling_left`, `fouling_reached` | names |
| `illness.moves` | Every camp moved, in order: the dawn, the places left and reached, the bodies whose home moved so, the place left's members and sick at the dawn before, and the fouling of both places then (body-nights): to read, later, whether those who leave a fouled camp fare better | rows |
| `learning.on` | Learning causes, believe without acting (D59, 27 November 2026), its own block, last: whether the content holds a `learning` section (`content/needs.ron`); `false`, and no other key, without it (or in the control run `--no-learning`). Its bands of will: `timid` under 20, `middle` 20 to 49, `willing` 50 and more, the will of the body at the step | bool |
| `learning.marks.*.own` | Marks lived, by problem (`ill_gut`, `ill_chest`): falling ill (`Event::FellIll`) | count |
| `learning.marks.*.kin` | The same: a kin (spouse, child under 16, parent) seen sick at its fire at dawn | count |
| `learning.noticed.*` | Times a body marked noticed its problem comes back (the draw by will), by band | count |
| `learning.saw_nothing.*` | Of them, the times nothing was there in each episode it laid side by side, by band | count |
| `learning.believed.*` | Beliefs come to by cross-checking (new, or another than the one held), by band of the one who came to it | count |
| `learning.living_by_band.*` | The living with a mind, by band, counted at each sample of `learning.seasons` and summed: what `noticed` and `believed` are read against | body-samples |
| `learning.born_by_verdict.*` | Beliefs come to, by verdict against the illness rule (the vivarium reads it; no body does): `true` (each suspect true or partly, one true), `partly`, `false` (no suspect true). A suspect is true when the content makes it raise the ailment: an old or settled camp for the gut (its fouling), the season whose percent is over 100, many at the fire for the chest, a cold night for the chest, someone sick at the fire (contagion); partly: the camp itself and the place drunk at for the gut (where the fouling is, but leaving for good is not the cure), some at the fire for the chest, many for the gut; false: what was eaten, and every other | count |
| `learning.renewed.lived` | Beliefs renewed by a mark whose episode held all their suspects, or by coming to the same again | count |
| `learning.renewed.heard` | Beliefs renewed by hearing the same told at the fire | count |
| `learning.countered` | Episodes lived without a held belief's suspects, short of making it fall | count |
| `learning.told` | Beliefs told at a fire (each teller, each belief, each night) | count |
| `learning.listeners` | The others present at those tellings, summed | count |
| `learning.copied` | Beliefs taken by a listener | count |
| `learning.copied_tradition` | Of them, taken without carrying the problem: by a child, the tradition | count |
| `learning.forgotten.*` | Beliefs let go, by why: `faded` (not renewed in `whole_seasons` + `half_seasons`), `countered` (`counter_examples` episodes without its suspects), `replaced` (by its holder's own new one, or one heard believed more). Beliefs gone with their holder's death are not counted | count |
| `learning.beliefs[].belief` | A belief by its content, the problem then its suspects joined by ` + `: `camp` (its camp, whichever), `camp_age:new`/`settled`/`old`, `season:spring` … `winter`, `crowd:few`/`some`/`many`, `cold_night`, `sick_at_fire`, `ate:<material>`, `drank_there` (where it drank, whichever); the most born first | text |
| `learning.beliefs[].verdict` | Its verdict, as in `born_by_verdict` | `true`, `partly`, `false` |
| `learning.beliefs[].kinds` | The kind of cause of each suspect, from the content (`place`, `season`, `weather`, `company`, `food`, `water`, `gesture`): recorded, read by no rule yet | names |
| `learning.beliefs[].born.*` | Times it was come to, by band | count |
| `learning.beliefs[].told` | Times it was told | count |
| `learning.beliefs[].copied` | Times it was taken | count |
| `learning.beliefs[].copied_tradition` | Of them, by the tradition | count |
| `learning.beliefs[].forgotten` | Times it was let go | count |
| `learning.seasons[].day` | A sample at each season's first day: the day (from 0) | day |
| `learning.seasons[].living` | The living then | count |
| `learning.seasons[].holders` | Of them, holding at least one belief | count |
| `learning.seasons[].own` | Beliefs held that their holder came to | count |
| `learning.seasons[].heard` | Beliefs held that their holder took from a teller | count |
| `learning.seasons[].tradition` | Of those heard, taken without carrying the problem | count |
| `learning.seasons[].children_tradition` | Of those, held by a child now (under `child_years`) | count |
| `learning.seasons[].by_verdict.*` | Beliefs held, by verdict | count |
| `learning.seasons[].by_content` | The twelve beliefs most held, with their holders | `[[text, count], …]` |
| `learning.acting.on` | Acting (D59, amended 27 November 2026): whether the content's `learning` section holds `acting`; `false` in `--learning-believe-only`, where only `moves` and `seasons` still count | bool |
| `learning.acting.minded` | Marks that agreed with a belief of a place (every suspect there) whose holder's will made it mean to leave (the chance a distressed body tries) | count |
| `learning.acting.would.*` | Marks that agreed with a belief whose suspects give a lever not built yet, by lever (`prepare`, `keep_apart`, `avoid`, `drink_elsewhere`, `otherwise`): where it would apply | count |
| `learning.acting.called` | Moves called from a belief, at a free nightfall, to the best place its caller's notes know that its belief does not accuse, no worse for food than `worse_permille` | count |
| `learning.acting.waited` | Times one meant to leave and knew no such place | count |
| `learning.acting.stayed` | Calls from a belief dropped: the caller stayed (its spouse would not go and its own balance said no) | count |
| `learning.acting.lessons_told` | Beliefs told at a fire as a lesson: a trial from them was kept | count |
| `learning.acting.mourned` | Episodes held at their gravest by a kin when their sick died of it | count |
| `learning.acting.trials.belief.*` | Trials from a belief's call: `begun`, then their end, `kept` (the ills its holder perceived in the season after at most `keep_percent` of the season before), `abandoned` (not fewer: its belief countered), `unjudged` (fewer than `judge_nights` dawns away, no ill before, or a new move ended it) | count |
| `learning.acting.trials.chance.*` | The same for trials by chance: a move for another reason by one carrying an ill problem; kept, it leaves a belief of the camp left | count |
| `learning.acting.moves.belief` | Moves home by grown bodies (their home's place changed) from a belief's call, the caller's or one followed | count |
| `learning.acting.moves.other` | Moves home by grown bodies for any other reason | count |
| `learning.acting.seasons[].day` | Each season's first day (from 0) | day |
| `learning.acting.seasons[].moves_belief` | Moves from a belief in the season | count |
| `learning.acting.seasons[].moves_other` | Moves for other reasons in the season | count |
| `learning.acting.seasons[].nights_at_leaving_belief` | The nights in a row the movers from a belief had slept at the camp they left, summed: divided by `moves_belief`, the camp's age at leaving | nights |
| `learning.acting.seasons[].nights_at_leaving_other` | The same for the other moves | nights |
| `learning.acting.seasons[].gut_ills` | Gut ills lived in the season (own marks) | count |
| `learning.acting.seasons[].person_days` | The living counted each day of the season, summed | body-days |
| `learning.acting.seasons[].gut_per_10k_person_days` | `gut_ills` per ten thousand `person_days`, rounded down | rate |
| `search.on` | The search for an absent kin (D56, amended 27 November 2026), its own block, last: whether the content holds a `search` section (`content/needs.ron`); `false`, and no other key, without it (or under `--no-search`) | boolean |
| `search.absences_noticed` | Close ones (spouse, parent, child) noticed absent at a dawn's fire, not seen for `search.after_nights` nights: once an absence | count |
| `search.searches_begun` | Of them, those a kin set off to search (the others: the last place known was the camp itself, or no kin grown and free) | count |
| `search.search_tiles_mean` | The straight distance from the searcher to the place it set off to (said, or last seen), its mean, rounded down | tiles |
| `search.found_alive` | Searches ended by seeing the absent alive | count |
| `search.found_dead` | Searches where the absent's body, or its infant lying alone, was seen | count |
| `search.not_found` | Searches ended with nothing seen: standing at the place, or stopped short of it | count |
| `search.not_found_stopped_short` | Of them, stopped short of the place (water, rock, no way) | count |
| `search.infants_taken` | Infants lying alone taken up by a searcher | count |
| `search.infants_brought_home` | Of them, carried to the searcher's camp | count |
| `search.infants_given_to_a_woman` | Of them, given there by a man to a grown woman | count |
| `search.infants_survived_a_season` | Infants brought home still alive a season (a quarter of `meta.days_per_year`) later | count |
| `search.infants_died_within_a_season` | Infants brought home dead before that | count |
| `search.infants_season_not_over` | Infants brought home alive when the run ended less than a season later | count |
| `search.records[].day` | One record an absence noticed (27 November 2026, a measure for the audit of useless searches): the day of the fire (from 0) | day |
| `search.records[].absent.id` | The absent's body | id |
| `search.records[].absent.age_years` | Its age at the probe's last daily look (null: never looked at) | years |
| `search.records[].absent.sex` | Its sex | `female`, `male` |
| `search.records[].kin.id` | The kin who went; with no search, the first kin who noticed (spouse, then parent, then child) | id |
| `search.records[].kin.age_years` | Its age, as the absent's | years |
| `search.records[].kin.sex` | Its sex | `female`, `male` |
| `search.records[].absent_is_kin_s` | What the absent is to that kin | `spouse`, `child`, `parent` |
| `search.records[].noticers` | The kin at that fire who noticed the same absent that dawn | count |
| `search.records[].nights_since_seen` | The nights since that kin last saw the absent | nights |
| `search.records[].absent_alive` | Whether the absent was alive then: the vivarium's knowledge, no body's | boolean |
| `search.records[].same_hearth` | Whether the absent's home and the kin's were then the same camp (within the brains' `HOME_TILES`); the vivarium reads both minds, a dead absent's home is the last the probe saw; null when unknown | boolean |
| `search.records[].homes_apart_tiles` | The distance between the two homes then, on the longer axis | tiles |
| `search.records[].other_hearth_days` | With another hearth: the days since the later of the two took its home (the probe looks once a day); null with the same hearth | days |
| `search.records[].said_where` | Whether the absent had said, at the last fire the two shared, where it was going | boolean |
| `search.records[].search_began` | Whether a kin set off | boolean |
| `search.records[].why_not` | With no search, why the first kin with a reason stayed: `last_known_place_is_the_camp`, `too_young`, `already_searching`, `holds_a_goal`, `moving_home`; null with a search | name |
| `search.records[].went_to` | With a search, what it went to | `place_said`, `last_seen` |
| `search.records[].to` | That place | `[x, y]` |
| `search.records[].to_tiles` | The distance from where the searcher set off to that place, on the longer axis | tiles |
| `search.records[].walked_tiles` | The tiles the searcher walked from setting off to the result (to the end of the run when none) | tiles |
| `search.records[].result` | How the search ended: `found_alive`, `found_dead`, `not_found` (standing at the place), `stopped_short`; null with no search, or one not ended | name |
| `search.records[].result_day` | The day of the result | day |
| `search.records[].found_where` | Found alive, where the absent stood: `at_searcher_camp`, `at_another_camp` (its own home, another hearth), `sleeping_out` (asleep away from both), `awake_out` | name |
| `search.records[].found_tiles_from_kin_camp` | Found alive: its distance then from the searcher's home | tiles |

## Beside the data: the lab's series file

The laboratory (D52) keeps a series in `<runs-dir>/<series>/serie.json` (the full layout is in
`tools/lab/README.md`). Its pages read the keys below there; `plan.answer`, `runs[].note` and
`runs[].ref` are optional, written by hand or by the AI session, and an older file without them
opens all the same.

Listed here, not in a table: the keys of the tables above are the data file's own, and a test
of the vivarium walks them.

- `plan.question`: the series' question, at the top of its page (text).
- `plan.answer`: the answer, one or two sentences written when the series ends (24 November
  2026); without it the page says "Réponse : pas encore écrite" (text).
- `runs[].note`: the run's name for a human eye, what makes it differ (`"3 kg · richesse 7,3 ·
  graine 1"`); the page adds the seed when the note does not say it. Without a note, the run is
  named by the settings that differ between the series' runs, and its seed (text).
- `runs[].ref`: the run it is compared with (24 November 2026): `"series/run"`, possibly a run of
  another series, or a run of the same series alone; its cells are coloured against that run's
  (text).

The series' table reads from each run's data: `people` (at the start, births, deaths and those of
hunger, alive at the end), `nights_apart` (the couples together at dawn: `together` over `dawns`),
`positions` with `years[].camp` (the median distance at noon, on the longer axis, of the bodies
to their tribe's camp of that year, else of the year before, in the spring of each year: the
first quarter of `meta.days_per_year`), `home_changes` (counted) and `meta.timing.wall_s`. A run
written before a key shows "—" in its column.

## Not in the file

- The chronicle (on the full page, `--page full`), the map's picture, the families' pages (worked out from `people`:
  mother, father, spouses, children), the ages table (worked out from `born_day` and `sex`).
- The camp's median position per day: the vivarium works it out once a year (`years[].camp`), not
  per day; the lab can take it from `positions` (the median of a tribe's bodies at noon). The
  presence grid (`presence`) and the positions (`positions`) are the two things sampled for the
  data alone, once a day at noon: where the living of each tribe were, and each body one by one.
