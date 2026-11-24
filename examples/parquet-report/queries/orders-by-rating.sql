select case rating when 1 then '1 star' else cast(rating as varchar) || ' stars' end as rating, count(*) as orders
from orders
group by rating
order by orders.rating desc;
