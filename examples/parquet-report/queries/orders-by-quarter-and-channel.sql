select year(day) || ' Q' || quarter(day) as quarter, channel, count(*) as orders
from orders
group by quarter, channel
order by quarter, channel;
