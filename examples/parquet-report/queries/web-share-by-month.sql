select strftime(day, '%Y-%m') as month,
       round(100.0 * count(*) filter (where channel = 'Web') / count(*), 1) as web_share
from orders
group by month
order by month;
