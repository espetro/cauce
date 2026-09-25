September 28, 2025 · 8 min read

For five years I wrote every database query in our codebase through the ORM, because "consistency." Last month I deleted 400 lines of the reporting module and replaced it with 90 lines of hand-written SQL. This post is about why that was the right call.

## The problem nobody mentions

ORMs are excellent at object-shaped reads: fetch a user, fetch their orders, hydrate the rows. Reporting queries are not object-shaped. They aggregate across six tables, pivot on a date column, and produce a flat table of numbers that maps to no model. Forcing that through an ORM gives you code like this:

```
rows = (
    session.query(Order)
    .join(Order.items)
    .filter(Order.created_at >= start)
    .group_by(func.date_trunc("month", Order.created_at))
    .with_entities(func.date_trunc("month", Order.created_at), func.sum(OrderItem.price))
    .all()
)
```

That generates a query the ORM had to be coaxed into — and when it is slow, you end up reading the SQL it produced anyway. The abstraction did not save you; it added an indirection layer to debug through.

## What replaced it

The replacement is a folder of `.sql` files loaded at startup, each returning a typed dataclass row:

```
-- monthly_revenue.sql
SELECT date_trunc('month', o.created_at) AS month,
       sum(oi.price_cents) / 100.0        AS revenue_usd
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
WHERE o.created_at >= :start
GROUP BY 1
ORDER BY 1;
```

The measurable result: the monthly revenue query went from 800 ms to 90 ms, and the code review diff was almost entirely deletions. Readability went up — the SQL is the spec, not an artefact you dump from a query builder and paste into a comment.

## The rule I landed on

My rule now: if the query's output shape has a model name, use the ORM; if the output is a report row, write SQL. ORMs earned their place; they just never belonged in the reporting module.