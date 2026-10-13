use sqlx::SqlitePool;

pub async fn init_db(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT UNIQUE NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS chapters(id INTEGER PRIMARY KEY, subject TEXT NOT NULL, name TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS subtopics(id INTEGER PRIMARY KEY, chapter_id INTEGER NOT NULL REFERENCES chapters(id), name TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS questions(id INTEGER PRIMARY KEY, subject TEXT NOT NULL, chapter_id INTEGER NOT NULL, subtopic_id INTEGER NOT NULL, qtype TEXT NOT NULL, stem TEXT NOT NULL, opt_a TEXT, opt_b TEXT, opt_c TEXT, opt_d TEXT, answer TEXT NOT NULL, difficulty TEXT NOT NULL, year INTEGER NOT NULL, shift TEXT NOT NULL, solution TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS papers(id TEXT PRIMARY KEY, user_id TEXT NOT NULL, title TEXT NOT NULL, duration_min INTEGER NOT NULL, created_at TEXT NOT NULL, share_token TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS paper_questions(paper_id TEXT NOT NULL, question_id INTEGER NOT NULL, ord INTEGER NOT NULL, PRIMARY KEY(paper_id, question_id));
         CREATE TABLE IF NOT EXISTS attempts(id TEXT PRIMARY KEY, paper_id TEXT NOT NULL, user_id TEXT NOT NULL, started_at TEXT NOT NULL, submitted_at TEXT, score INTEGER DEFAULT 0, max_score INTEGER DEFAULT 0, used_secs INTEGER DEFAULT 0, right_count INTEGER DEFAULT 0, wrong_count INTEGER DEFAULT 0);
         CREATE TABLE IF NOT EXISTS answers(attempt_id TEXT NOT NULL, question_id INTEGER NOT NULL, selected TEXT DEFAULT '', marked_review INTEGER DEFAULT 0, time_secs INTEGER DEFAULT 0, PRIMARY KEY(attempt_id, question_id));
         CREATE TABLE IF NOT EXISTS mistake_tags(attempt_id TEXT NOT NULL, question_id INTEGER NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(attempt_id, question_id, tag));
         CREATE TABLE IF NOT EXISTS sessions(token TEXT PRIMARY KEY, user_id TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS pkce(state TEXT PRIMARY KEY, verifier TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS seen_questions(user_id TEXT NOT NULL, question_id INTEGER NOT NULL, seen_at TEXT NOT NULL DEFAULT '', PRIMARY KEY(user_id, question_id));
         CREATE INDEX IF NOT EXISTS idx_questions_chapter ON questions(chapter_id);
         CREATE INDEX IF NOT EXISTS idx_questions_subtopic ON questions(subtopic_id);
         CREATE INDEX IF NOT EXISTS idx_questions_subject ON questions(subject);
         CREATE INDEX IF NOT EXISTS idx_questions_year ON questions(year);
         CREATE INDEX IF NOT EXISTS idx_subtopics_chapter ON subtopics(chapter_id);
         CREATE INDEX IF NOT EXISTS idx_papers_user ON papers(user_id);
         CREATE INDEX IF NOT EXISTS idx_attempts_paper ON attempts(paper_id);
         CREATE INDEX IF NOT EXISTS idx_attempts_user ON attempts(user_id);
         CREATE INDEX IF NOT EXISTS idx_answers_attempt ON answers(attempt_id);
         CREATE INDEX IF NOT EXISTS idx_papers_token ON papers(share_token);
         CREATE INDEX IF NOT EXISTS idx_mistakes_attempt ON mistake_tags(attempt_id);",
    )
    .execute(pool)
    .await?;
    // Migrate pre-seen_at databases: CREATE TABLE IF NOT EXISTS cannot add
    // the column to an existing table, so ALTER it in before creating the
    // index that references it.
    {
        use sqlx::Row;
        let cols: Vec<String> = sqlx::query("PRAGMA table_info(seen_questions)")
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();
        if !cols.iter().any(|c| c == "seen_at") {
            sqlx::query("ALTER TABLE seen_questions ADD COLUMN seen_at TEXT NOT NULL DEFAULT ''")
                .execute(pool)
                .await
                .ok();
        }
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_seen_user_time ON seen_questions(user_id, seen_at)")
            .execute(pool)
            .await
            .ok();

        let attempt_cols: Vec<String> = sqlx::query("PRAGMA table_info(attempts)")
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();
        if !attempt_cols.iter().any(|c| c == "right_count") {
            sqlx::query("ALTER TABLE attempts ADD COLUMN right_count INTEGER DEFAULT 0")
                .execute(pool)
                .await
                .ok();
        }
        if !attempt_cols.iter().any(|c| c == "wrong_count") {
            sqlx::query("ALTER TABLE attempts ADD COLUMN wrong_count INTEGER DEFAULT 0")
                .execute(pool)
                .await
                .ok();
        }
    }
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM questions")
        .fetch_one(pool)
        .await?;
    if n == 0 {
        seed(pool).await?;
    }
    Ok(())
}

struct SeedQ<'a> {
    subject: &'a str,
    chapter: &'a str,
    subtopic: &'a str,
    qtype: &'a str,
    stem: &'a str,
    opts: [&'a str; 4],
    answer: &'a str,
    difficulty: &'a str,
    year: i64,
    shift: &'a str,
    solution: &'a str,
}

pub async fn seed(pool: &SqlitePool) -> anyhow::Result<()> {
    let chapters: Vec<(&str, &str, Vec<&str>)> = vec![
        ("Physics", "Mechanics", vec!["Kinematics", "Laws of Motion", "Work Energy Power"]),
        ("Physics", "Electromagnetism", vec!["Electrostatics", "Current Electricity"]),
        ("Physics", "Optics", vec!["Ray Optics", "Wave Optics"]),
        ("Chemistry", "Physical Chemistry", vec!["Mole Concept", "Chemical Kinetics"]),
        ("Chemistry", "Organic Chemistry", vec!["GOC", "Aldehydes and Ketones"]),
        ("Chemistry", "Inorganic Chemistry", vec!["Periodic Table", "Coordination Compounds"]),
        ("Maths", "Algebra", vec!["Quadratic Equations", "Matrices and Determinants"]),
        ("Maths", "Calculus", vec!["Limits and Continuity", "Definite Integration"]),
        ("Maths", "Geometry", vec!["Straight Lines", "Parabola"]),
    ];
    for (subj, ch, subs) in &chapters {
        use sqlx::Row;
    let id: i64 = sqlx::query("INSERT INTO chapters(subject,name) VALUES(?,?) RETURNING id")
            .bind(subj)
            .bind(ch)
            .fetch_one(pool)
            .await?
            .get(0);
        for s in subs {
            sqlx::query("INSERT INTO subtopics(chapter_id,name) VALUES(?,?)")
                .bind(id)
                .bind(s)
                .execute(pool)
                .await?;
        }
    }
    let qs: Vec<SeedQ> = vec![
        SeedQ{subject:"Physics",chapter:"Mechanics",subtopic:"Kinematics",qtype:"MCQ",stem:"A particle starts from rest with constant acceleration 2 m/s2. Distance covered in the 5th second is:",opts:["9 m","8 m","10 m","5 m"],answer:"A",difficulty:"Easy",year:2024,shift:"S1",solution:"Sn = u + a(2n-1)/2 = 0 + 2(9)/2 = 9 m."},
        SeedQ{subject:"Physics",chapter:"Mechanics",subtopic:"Kinematics",qtype:"MCQ",stem:"Velocity-time graph is a straight line parallel to time axis. The motion is:",opts:["uniform velocity","uniform acceleration","non-uniform velocity","at rest"],answer:"A",difficulty:"Easy",year:2023,shift:"S2",solution:"Slope zero means acceleration zero, velocity constant."},
        SeedQ{subject:"Physics",chapter:"Mechanics",subtopic:"Laws of Motion",qtype:"MCQ",stem:"A 2 kg block on a frictionless surface is pulled by 10 N. Its acceleration is:",opts:["5 m/s2","20 m/s2","2 m/s2","0.2 m/s2"],answer:"A",difficulty:"Easy",year:2024,shift:"S1",solution:"a = F/m = 10/2 = 5 m/s2."},
        SeedQ{subject:"Physics",chapter:"Mechanics",subtopic:"Work Energy Power",qtype:"NUM",stem:"A 1 kg mass drops from 20 m (g=10). Speed just before impact in m/s is:",opts:["","","",""],answer:"20",difficulty:"Medium",year:2022,shift:"S1",solution:"v = sqrt(2gh) = sqrt(400) = 20 m/s."},
        SeedQ{subject:"Physics",chapter:"Electromagnetism",subtopic:"Electrostatics",qtype:"MCQ",stem:"Force between two point charges doubles the distance. The force becomes:",opts:["one-fourth","half","double","same"],answer:"A",difficulty:"Easy",year:2023,shift:"S1",solution:"Coulomb force scales as 1/r2."},
        SeedQ{subject:"Physics",chapter:"Electromagnetism",subtopic:"Current Electricity",qtype:"MCQ",stem:"A wire of resistance R is stretched to double length. New resistance is:",opts:["4R","2R","R/2","R"],answer:"A",difficulty:"Medium",year:2024,shift:"S2",solution:"R = rho l/A; volume constant gives R proportional to l2, so 4R."},
        SeedQ{subject:"Physics",chapter:"Optics",subtopic:"Ray Optics",qtype:"MCQ",stem:"Focal length of an equiconvex lens (n=1.5, R=20 cm) is:",opts:["20 cm","40 cm","10 cm","30 cm"],answer:"A",difficulty:"Medium",year:2022,shift:"S2",solution:"1/f = (n-1)(2/R) = 0.5(2/20) = 1/20."},
        SeedQ{subject:"Physics",chapter:"Optics",subtopic:"Wave Optics",qtype:"MCQ",stem:"In YDSE, fringe width is proportional to:",opts:["wavelength","1/wavelength","screen distance squared","slit width"],answer:"A",difficulty:"Easy",year:2023,shift:"S1",solution:"beta = lambda D/d."},
        SeedQ{subject:"Chemistry",chapter:"Physical Chemistry",subtopic:"Mole Concept",qtype:"MCQ",stem:"Moles in 4 g of NaOH (M=40) are:",opts:["0.1","0.4","1","4"],answer:"A",difficulty:"Easy",year:2024,shift:"S1",solution:"n = 4/40 = 0.1."},
        SeedQ{subject:"Chemistry",chapter:"Physical Chemistry",subtopic:"Chemical Kinetics",qtype:"MCQ",stem:"For a first order reaction, t1/2 = 10 min. Rate constant k is:",opts:["0.0693 min-1","0.693 min-1","10 min-1","1 min-1"],answer:"A",difficulty:"Medium",year:2023,shift:"S2",solution:"k = 0.693/t1/2 = 0.0693 min-1."},
        SeedQ{subject:"Chemistry",chapter:"Organic Chemistry",subtopic:"GOC",qtype:"MCQ",stem:"Most stable carbocation among: methyl, ethyl, isopropyl, tert-butyl:",opts:["tert-butyl","methyl","ethyl","isopropyl"],answer:"A",difficulty:"Easy",year:2022,shift:"S1",solution:"Hyperconjugation plus inductive effect: 3 degree most stable."},
        SeedQ{subject:"Chemistry",chapter:"Organic Chemistry",subtopic:"Aldehydes and Ketones",qtype:"MCQ",stem:"Tollens reagent gives silver mirror with:",opts:["aldehydes","ketones","alkenes","ethers"],answer:"A",difficulty:"Easy",year:2024,shift:"S2",solution:"Aldehydes reduce Tollens; ketones do not."},
        SeedQ{subject:"Chemistry",chapter:"Inorganic Chemistry",subtopic:"Periodic Table",qtype:"MCQ",stem:"Highest first ionisation enthalpy among second period:",opts:["Ne","Li","B","O"],answer:"A",difficulty:"Easy",year:2023,shift:"S1",solution:"Noble gas Ne has maximum IE1 in period 2."},
        SeedQ{subject:"Chemistry",chapter:"Inorganic Chemistry",subtopic:"Coordination Compounds",qtype:"MCQ",stem:"Oxidation state of Fe in [Fe(CN)6]4- is:",opts:["+2","+3","+4","0"],answer:"A",difficulty:"Medium",year:2024,shift:"S1",solution:"x + 6(-1) = -4 gives x = +2."},
        SeedQ{subject:"Maths",chapter:"Algebra",subtopic:"Quadratic Equations",qtype:"MCQ",stem:"Roots of x2 - 5x + 6 = 0 are:",opts:["2, 3","1, 6","-2, -3","5, 1"],answer:"A",difficulty:"Easy",year:2024,shift:"S1",solution:"Factors (x-2)(x-3) = 0."},
        SeedQ{subject:"Maths",chapter:"Algebra",subtopic:"Matrices and Determinants",qtype:"NUM",stem:"Determinant of [[2,0],[0,3]] is:",opts:["","","",""],answer:"6",difficulty:"Easy",year:2023,shift:"S1",solution:"Diagonal product 2x3 = 6."},
        SeedQ{subject:"Maths",chapter:"Calculus",subtopic:"Limits and Continuity",qtype:"MCQ",stem:"lim x->0 sin(x)/x equals:",opts:["1","0","infinity","-1"],answer:"A",difficulty:"Easy",year:2022,shift:"S2",solution:"Standard limit equals 1."},
        SeedQ{subject:"Maths",chapter:"Calculus",subtopic:"Definite Integration",qtype:"MCQ",stem:"Integral 0 to pi of sin(x) dx equals:",opts:["2","0","1","pi"],answer:"A",difficulty:"Medium",year:2024,shift:"S2",solution:"[-cos x]0^pi = 1 + 1 = 2."},
        SeedQ{subject:"Maths",chapter:"Geometry",subtopic:"Straight Lines",qtype:"MCQ",stem:"Slope of line 2x + 3y = 6 is:",opts:["-2/3","2/3","3/2","-3/2"],answer:"A",difficulty:"Easy",year:2023,shift:"S2",solution:"y = -2x/3 + 2, slope -2/3."},
        SeedQ{subject:"Maths",chapter:"Geometry",subtopic:"Parabola",qtype:"MCQ",stem:"Focus of y2 = 8x is:",opts:["(2,0)","(0,2)","(4,0)","(8,0)"],answer:"A",difficulty:"Medium",year:2024,shift:"S1",solution:"4a = 8 gives a = 2, focus (a,0)."},
        SeedQ{subject:"Physics",chapter:"Mechanics",subtopic:"Laws of Motion",qtype:"MCQ",stem:"A car rounds a curve at constant speed. Its acceleration is:",opts:["centripetal, nonzero","zero","tangential only","infinite"],answer:"A",difficulty:"Medium",year:2023,shift:"S2",solution:"Direction changes so velocity changes; centripetal acceleration nonzero."},
        SeedQ{subject:"Chemistry",chapter:"Physical Chemistry",subtopic:"Mole Concept",qtype:"NUM",stem:"Molarity of 0.5 mol solute in 2 L solution is (mol/L):",opts:["","","",""],answer:"0.25",difficulty:"Easy",year:2022,shift:"S1",solution:"M = 0.5/2 = 0.25."},
        SeedQ{subject:"Maths",chapter:"Algebra",subtopic:"Quadratic Equations",qtype:"MCQ",stem:"Discriminant of x2 + 4x + 4 is:",opts:["0","16","-16","4"],answer:"A",difficulty:"Easy",year:2023,shift:"S1",solution:"D = 16 - 16 = 0, equal roots."},
        SeedQ{subject:"Maths",chapter:"Calculus",subtopic:"Definite Integration",qtype:"NUM",stem:"Integral 0 to 1 of 2x dx equals:",opts:["","","",""],answer:"1",difficulty:"Easy",year:2022,shift:"S1",solution:"[x2]0^1 = 1."},
    ];
    use sqlx::Row;
    for q in &qs {
        let ch_id: i64 = sqlx::query("SELECT id FROM chapters WHERE subject=? AND name=?")
            .bind(q.subject)
            .bind(q.chapter)
            .fetch_one(pool)
            .await?
            .get(0);
        let st_id: i64 = sqlx::query("SELECT id FROM subtopics WHERE chapter_id=? AND name=?")
            .bind(ch_id)
            .bind(q.subtopic)
            .fetch_one(pool)
            .await?
            .get(0);
        sqlx::query("INSERT INTO questions(subject,chapter_id,subtopic_id,qtype,stem,opt_a,opt_b,opt_c,opt_d,answer,difficulty,year,shift,solution) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(q.subject).bind(ch_id).bind(st_id).bind(q.qtype).bind(q.stem)
            .bind(q.opts[0]).bind(q.opts[1]).bind(q.opts[2]).bind(q.opts[3])
            .bind(q.answer).bind(q.difficulty).bind(q.year).bind(q.shift).bind(q.solution)
            .execute(pool).await?;
    }
    Ok(())
}
