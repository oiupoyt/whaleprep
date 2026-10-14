#!/usr/bin/env python3
"""Reorganize the question bank to match the official NTA JEE Main syllabus units.

NTA 2026 Paper 1 (B.E./B.Tech) units:
  Mathematics (14): Sets Relations & Functions | Complex Numbers & Quadratic Equations |
    Matrices & Determinants | Permutations & Combinations | Binomial Theorem |
    Sequence & Series | Limit Continuity & Differentiability | Integral Calculus |
    Differential Equations | Co-ordinate Geometry | 3D Geometry | Vector Algebra |
    Statistics & Probability | Trigonometry
  Physics (20): Units & Measurements | Kinematics | Laws of Motion | Work Energy & Power |
    Rotational Motion | Gravitation | Properties of Solids & Liquids | Thermodynamics |
    Kinetic Theory of Gases | Oscillations & Waves | Electrostatics | Current Electricity |
    Magnetic Effects of Current & Magnetism | EMI & Alternating Currents |
    Electromagnetic Waves | Optics | Dual Nature of Matter & Radiation | Atoms & Nuclei |
    Electronic Devices | Experimental Skills
  Chemistry (20+1): Some Basic Concepts | Atomic Structure | Chemical Bonding |
    Chemical Thermodynamics | Solutions | Equilibrium | Redox & Electrochemistry |
    Chemical Kinetics | Classification of Elements & Periodicity | p-Block |
    d- and f-Block | Coordination Compounds | Purification & Characterisation of Organic Compounds |
    Basic Principles of Organic Chemistry | Hydrocarbons | Organic Compounds Containing Halogens |
    Organic Compounds Containing Oxygen | Organic Compounds Containing Nitrogen |
    Biomolecules | Principles Related to Practical Chemistry

Maps by KEYWORD against current subtopic names. A subtopic is assigned to the
first unit whose keyword pattern matches. Unmatched subtopics stay in their
current chapter (and are reported for manual review).
"""
import sqlite3
import sys

# (NTA unit name, [keyword patterns to match against subtopic names])
MATHS_UNITS = [
    ("Sets, Relations and Functions", ["set", "relation", "venn", "function composition", "one-one, into"]),
    ("Complex Numbers and Quadratic Equations", ["complex number", "quadratic", "cube roots of unity", "nth roots of unity", "de moivres", "argand"]),
    ("Matrices and Determinants", ["matrix", "matrices", "determinant", "adjoint", "linear equations in"]),
    ("Permutations and Combinations", ["permutation", "combination", "factorial", "circular permutation"]),
    ("Binomial Theorem and Its Simple Applications", ["binomial theorem", "general term", "middle term", "multinomial", "binomial co"]),
    ("Sequence and Series", ["arithmetic progression", "geometric progression", "am gm", "harmonic progression", "summation of series", "arithmetico"]),
    ("Limit, Continuity and Differentiability", ["limit", "continuity", "differentiab", "maxima", "minima", "monotonic", "tangent and normal", "mean value theorem", "rate of change", "composite function", "domain", "range", "functional equation", "periodic", "even and odd", "inverse function", "newton lebnitz", "successive differentiation", "methods of differentiation"]),
    ("Integral Calculus", ["integral", "integration", "area bounded", "area under"]),
    ("Differential Equations", ["differential equation", "order and degree", "formation of differential"]),
    ("Co-ordinate Geometry", ["straight line", "circle", "parabola", "ellipse", "hyperbola", "chord", "tangent to", "normal to", "locus", "section formula", "distance formula", "angle between two lines", "family of circle", "radical axis", "concurrency", "collinearity", "centers of triangle", "common tangent", "pair of straight", "position of a point", "intercepts of a circle", "pair of tangents", "director circle", "distance of a point from a line", "various forms of straight"]),
    ("Three Dimensional Geometry", ["3d", "three dimension", "lines in space", "plane in space", "skew", "direction cosine", "lines and plane"]),
    ("Vector Algebra", ["vector", "dot product", "cross product", "scalar and vector"]),
    ("Statistics and Probability", ["probability", "standard deviation", "variance", "mean deviation", "mean median", "bayes", "binomial distribution"]),
    ("Trigonometry", ["trigonometric", "inverse trigonometric", "height and distance", "sine rule", "cosine rule", "circumcenter", "incenter", "orthocenter", "ex circle", "median and angle bisector", "half angle"]),
]

PHYSICS_UNITS = [
    ("Units and Measurements", ["units and measurements", "dimension", "significant figure", "error", "vernier", "screw gauge"]),
    ("Kinematics", ["average speed", "instantaneous speed", "average velocity", "instantaneous velocity", "average acceleration", "instantaneous acceleration", "constant acceleration", "variable acceleration", "projectile", "uniform circular", "non uniform circular", "relative motion", "rain man", "river boat", "motion under gravity", "motion on an inclined", "motion of connected", "vector arithmetic", "vector projection", "dot product", "cross product", "kinematics"]),
    ("Laws of Motion", ["newtons law", "friction", "equilibrium of a particle", "spring force", "impulse momentum", "free body", "cavity problem"]),
    ("Work, Energy and Power", ["work energy power", "energy", "power", "work"]),
    ("Rotational Motion", ["moment of inertia", "angular momentum", "torque", "center of mass", "combined translational and rotational", "collision"]),
    ("Gravitation", ["gravitation", "escape speed", "keplers law", "acceleration due to gravity"]),
    ("Properties of Solids and Liquids", ["mechanical properties", "stress strain", "surface tension", "excess pressure", "capillarity", "pressure density", "pascals law", "archimedes", "fluid flow", "bernoulli", "viscosity"]),
    ("Thermodynamics", ["thermodynamics process", "zeroth and first law", "heat engine", "carnot", "specific heat capacity", "calorimetry", "change of state", "thermometry", "thermal expansion", "newtons law of cooling", "heat transfer"]),
    ("Kinetic Theory of Gases", ["kinetic theory of gases", "gas laws", "degree of freedom", "equipartition"]),
    ("Oscillations and Waves", ["simple harmonic", "shm", "oscillation", "resonance", "progressive wave", "doppler", "superposition", "reflection of wave", "standing wave"]),
    ("Electrostatics", ["electric charge", "coulombs law", "electric field", "electric dipole", "electric flux", "gauss law", "electric potential", "capacitance", "capacitor", "dielectric", "electrostatics", "conductor"]),
    ("Current Electricity", ["current electricity", "ohms law", "resistance", "resistivity", "color coding", "electric cell", "battery", "kirchhoff", "wheatstone", "meter bridge", "potentiometer", "galvanometer", "voltmeter", "ammeter", "electric power", "heating effect", "drift velocity", "current density", "combination of resistance"]),
    ("Magnetic Effects of Current and Magnetism", ["biot savart", "amperes circuital", "magnetic field", "bar magnet", "magnetic dipole", "magnetic moment", "magnetic properties", "earth magnetism", "force and torque on current", "moving charge", "moving coil"]),
    ("Electromagnetic Induction and Alternating Currents", ["faradays", "lenzs law", "magnetic flux", "inductance", "motional emf", "eddy current", "growth and decay of current", "ac circuit", "ac generator", "transformer", "quality factor", "rc circuit"]),
    ("Electromagnetic Waves", ["electromagnetic wave", "em wave", "displacement current", "em spectrum", "maxwell"]),
    ("Optics", ["reflection of light", "refraction", "prism", "lense", "optical instrument", " huygens", "interference of light", "diffraction", "polarisation", "doppler effect of light", "ray optics", "wave optics"]),
    ("Dual Nature of Matter and Radiation", ["photoelectric effect", "particle nature of light", "photon", "matter wave", "davisson and germer", "de broglie"]),
    ("Atoms and Nuclei", ["bohrs model", "hydrogen spectrum", "alpha particle scattering", "rutherford model of atom", "nucleus", "radioactivity", "nuclear fission", "nuclear fusion", "binding energy"]),
    ("Electronic Devices", ["semiconductor", "p n junction", "diode", "transistor", "digital circuit", "logic gate"]),
    ("Experimental Skills", ["communication system", "propagation of em wave", "modulation", "demodulation"]),
]

CHEMISTRY_UNITS = [
    ("Some Basic Concepts in Chemistry", ["mole concept", "concentration terms", "laws of chemical combination", "significant figures", "quantitative measures in chemical"]),
    ("Atomic Structure", ["bohrs model for hydrogen atom", "de broglie hypothesis", "heisenberg", "hydrogen spectrum", "quantum number", "quantum mechanical model", "electronic configuration and nodes", "thomsons model", "rutherfords model", "atomic structure", "schrodinger"]),
    ("Chemical Bonding and Molecular Structure", ["chemical bonding", "covalent", "coordinate bond", "ionic bond", "lewis theory", "vsepr", "hybridization", "molecular orbital", "resonance", "dipole moment", "bond parameter", "hydrogen bonding", "back bonding", "van der walls"]),
    ("Chemical Thermodynamics", ["thermodynamics", "enthalp", "hesss law", "entropy", "free energy", "spontaneity", "first law of thermodynamics", "fundamentals of thermodynamics"]),
    ("Solutions", ["solution", "colligative", "vant hoff", "osmotic pressure", "raoults", "roults", "elevation in boiling", "depression in freezing", "relative lowering", "henrys law"]),
    ("Equilibrium", ["chemical equilibrium", "ionic equilibrium", "le chateliers", "equilibrium constant", "ostwalds dilution", "ph buffer", "salt hydrolysis", "solubility product", "common ion effect"]),
    ("Redox Reactions and Electrochemistry", ["redox", "electrochem", "oxidation", "reduction", "galvanic", "cell and their emf", "conductance", "electrolysis", "nernst"]),
    ("Chemical Kinetics", ["chemical kinetics", "rate of reaction", "rate law", "rate constant", "arrhenius", "order of reaction", "integrated rate", "nuclear chemistry"]),
    ("Classification of Elements and Periodicity in Properties", ["periodic table", "periodicity", "atomic and ionic radius", "ionization energy", "electron gain enthalpy", "electronegativity", "effective nuclear charge", "nature of oxide"]),
    ("p-Block Elements", ["p-block", "group 13", "group 14", "group 15", "group 16", "group 17", "group 18", "boron family", "carbon family", "nitrogen family", "oxygen family", "halogen family", "noble gas", "inert gas"]),
    ("d- and f-Block Elements", ["d-block", "f-block", "transition element", "lanthanoid", "actinoid", "inner transition"]),
    ("Coordination Compounds", ["coordination compound", "coordination number", "crystal field theory", "cft", "isomerism of coordination", "isomerisom of coordination", "nomenclature of coordination", "warners theory", "valence bond theory", "organometallic"]),
    ("Purification and Characterisation of Organic Compounds", ["purification of organic", "iupac nomenclature", "detection of extra elements", "qualitative salt analysis", "chromatography", "crystallization", "distillation", "sublimation"]),
    ("Some Basic Principles of Organic Chemistry", ["basic principles of organic", "goc", "electron displacement", "isomerism", "acid and base strength", "stability of intermediate", "aromaticity", "inductive effect", "mesomeric", "hyperconjugation", "electrophile", "nucleophile"]),
    ("Hydrocarbons", ["hydrocarbon", "alkane", "alkene", "alkyne", "aromatic hydrocarbon", "petroleum"]),
    ("Organic Compounds Containing Halogens", ["haloalkane", "haloarene", "halogen", "polyhalogen", "alkyl halide", "aryl halide"]),
    ("Organic Compounds Containing Oxygen", ["alcohol", "phenol", "ether", "aldehyde", "ketone", "carboxylic acid", "grignard", "ester", "acid anhydride", "acyl chloride"]),
    ("Organic Compounds Containing Nitrogen", ["amine", "diazonium", "nitro", "nitrile", "amide", "cyanide"]),
    ("Biomolecules", ["biomolecule", "carbohydrate", "protein", "enzyme", "vitamin", "nucleic acid", "lipid", "amino acid"]),
    ("Principles Related to Practical Chemistry", ["practical chemistry", "titrimetric", "chemicals in foods", "chemicals in medicines", "cleansing agent", "chemical principle", "flame and charcoal", "preliminary test", "systematic analysis of anion", "systematic analysis of cation", "introduction to qualitative analysis", "preparation of organic and inorganic compound", "experiments involving physical chemistry"]),
]

ALL_UNITS = {
    "Maths": MATHS_UNITS,
    "Physics": PHYSICS_UNITS,
    "Chemistry": CHEMISTRY_UNITS,
}

def match_subtopic(subtopic_name, units):
    """Return the NTA unit whose first keyword pattern matches the subtopic name."""
    low = subtopic_name.lower()
    for unit_name, patterns in units:
        for pat in patterns:
            if pat in low:
                return unit_name
    return None

def get_or_create_chapter(cur, subject, name):
    row = cur.execute("SELECT id FROM chapters WHERE subject=? AND name=?", (subject, name)).fetchone()
    if row:
        return row[0]
    cur.execute("INSERT INTO chapters(subject, name) VALUES(?, ?)", (subject, name))
    return cur.lastrowid

def main(db_path):
    con = sqlite3.connect(db_path)
    cur = con.cursor()

    print(f"Reorganizing {db_path} to NTA JEE Main syllabus units...")

    # Create all NTA unit chapters
    for subj, units in ALL_UNITS.items():
        for unit_name, _ in units:
            get_or_create_chapter(cur, subj, unit_name)

    # Map every subtopic to an NTA unit
    moved, unmoved = 0, []
    rows = cur.execute("""
        SELECT s.id, s.name, c.subject, c.name
        FROM subtopics s JOIN chapters c ON c.id = s.chapter_id
    """).fetchall()

    for sid, sname, subj, current_ch in rows:
        units = ALL_UNITS.get(subj, [])
        target = match_subtopic(sname, units)
        if target and target != current_ch:
            tid = get_or_create_chapter(cur, subj, target)
            cur.execute("UPDATE subtopics SET chapter_id=? WHERE id=?", (tid, sid))
            moved += 1
        elif not target:
            unmoved.append((subj, current_ch, sname))

    # Sync question.chapter_id
    cur.execute("""
        UPDATE questions
        SET chapter_id = (SELECT chapter_id FROM subtopics WHERE subtopics.id = questions.subtopic_id)
    """)
    con.commit()

    # Report
    print(f"\nMoved {moved} subtopics to their NTA units.")
    print(f"{len(unmoved)} subtopics had no matching unit pattern (left in place):")
    for subj, ch, sname in unmoved[:20]:
        print(f"  [{subj}] {ch} -> {sname}")

    print("\n=== FINAL STRUCTURE ===")
    for r in cur.execute("""
        SELECT c.subject, c.name,
               (SELECT count(*) FROM subtopics s WHERE s.chapter_id=c.id),
               (SELECT count(*) FROM questions q WHERE q.chapter_id=c.id)
        FROM chapters c
        WHERE (SELECT count(*) FROM questions q WHERE q.chapter_id=c.id) > 0
        ORDER BY c.subject, c.name
    """).fetchall():
        print(f"  {r[0]:<10} | {r[1]:<45} | {r[2]:>2} subs | {r[3]:>5} qs")

    con.close()

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "whaleprep.db")
