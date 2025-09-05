use cella_lib::*;

fn center_apply(sub: &Rule2DSubrule, current: &CellType, crit: &CellType, points: &[(i32,i32)]) -> Option<CellType> {
    // Build a small neighbor set from a list of relative offsets
    use std::collections::HashSet;
    let set: HashSet<(i32,i32)> = points.iter().copied().collect();
    sub.applies_and_output(current, |dx, dy| {
        if set.contains(&(dx, dy)) { crit.clone() } else { CellType::inactive() }
    })
}

#[test]
fn countop_validation_success_variants() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    // eq only valid without limit
    let eq_ok = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert!(eq_ok.validate().is_ok());

    // gt with no limit
    let gt_ok = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert!(gt_ok.validate().is_ok());

    // lt with no limit
    let lt_ok = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Lt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert!(lt_ok.validate().is_ok());

    // between via gt with upper inclusive limit
    let between_gt = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: Some(3), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert!(between_gt.validate().is_ok());

    // between via lt with lower inclusive limit
    let between_lt = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Lt, limit: Some(2), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert!(between_lt.validate().is_ok());
}

#[test]
fn countop_validation_failure_variants() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    // eq with limit is invalid
    let eq_bad = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Eq, limit: Some(3), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert_eq!(eq_bad.validate(), Err(RuleError::InvalidRange2D));

    // gt with limit < count invalid
    let gt_bad = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Gt, limit: Some(2), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert_eq!(gt_bad.validate(), Err(RuleError::InvalidRange2D));

    // lt with limit > count invalid
    let lt_bad = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Lt, limit: Some(3), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert_eq!(lt_bad.validate(), Err(RuleError::InvalidRange2D));

    // range must be >=1
    let range_bad = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 0, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
    assert_eq!(range_bad.validate(), Err(RuleError::InvalidRange2D));

    // randomness in [0,1]
    let rand_bad = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: Some(1.5), output_type: b.clone() };
    assert_eq!(rand_bad.validate(), Err(RuleError::InvalidRandomness));
}

#[test]
fn countop_applies_eq_exact() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let out = CellType("O".into());
    let sub = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: out.clone() };
    // Provide exactly three neighbors
    let res = center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]);
    assert_eq!(res, Some(out.clone()));
    // Two neighbors should not pass
    let res2 = center_apply(&sub, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res2, None);
}

#[test]
fn countop_applies_gt_and_lt() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let outg = CellType("OG".into());
    let outl = CellType("OL".into());
    let gt = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: outg.clone() };
    let lt = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Lt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: outl.clone() };
    // gt: 2 neighbors should pass, 1 should not
    let res_gt2 = center_apply(&gt, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res_gt2, Some(outg.clone()));
    let res_gt1 = center_apply(&gt, &a, &b, &[(-1,0)]);
    assert_eq!(res_gt1, None);
    // lt: 1 neighbor should pass, 2 should not (since <=1)
    let res_lt1 = center_apply(&lt, &a, &b, &[(-1,0)]);
    assert_eq!(res_lt1, Some(outl.clone()));
    let res_lt2 = center_apply(&lt, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res_lt2, None);
}

#[test]
fn countop_applies_between_gt() {
    // between via Gt: count..=limit
    let a = CellType("A".into());
    let b = CellType("B".into());
    let out = CellType("OB".into());
    let sub = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: Some(3), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: out.clone() };
    // 2 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0)]), Some(out.clone()));
    // 3 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]), Some(out.clone()));
    // 1 neighbor -> fail
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0)]), None);
    // 4 neighbors -> fail
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1),(0,-1)]), None);
}

#[test]
fn countop_applies_between_lt() {
    // between via Lt: limit..=count
    let a = CellType("A".into());
    let b = CellType("B".into());
    let out = CellType("OC".into());
    let sub = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Lt, limit: Some(2), range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: out.clone() };
    // 2 neighbors -> pass (>=2 and <=3)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0)]), Some(out.clone()));
    // 3 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]), Some(out.clone()));
    // 1 neighbor -> fail (below lower bound 2)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0)]), None);
    // 4 neighbors -> fail (above upper count 3)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1),(0,-1)]), None);
}
