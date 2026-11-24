select strftime(day, '%Y-%m') as month, sum(revenue) as revenue
from orders
group by month
order by month;
